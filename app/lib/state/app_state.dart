import 'dart:async';
import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../src/rust/api/mumbleway.dart';

/// A server the user saved, persisted between launches.
class SavedServer {
  SavedServer({
    required this.name,
    required this.host,
    required this.port,
    required this.username,
    this.password,
    this.certFingerprint,
  });

  final String name;
  final String host;
  final int port;
  final String username;
  final String? password;
  final String? certFingerprint;

  /// Matches the id the Rust core derives, so the two stay in step.
  String get id => '$host:$port';

  SavedServer copyWith({String? certFingerprint}) => SavedServer(
        name: name,
        host: host,
        port: port,
        username: username,
        password: password,
        certFingerprint: certFingerprint ?? this.certFingerprint,
      );

  Map<String, dynamic> toJson() => {
        'name': name,
        'host': host,
        'port': port,
        'username': username,
        'password': password,
        'certFingerprint': certFingerprint,
      };

  static SavedServer fromJson(Map<String, dynamic> j) => SavedServer(
        name: j['name'] as String? ?? '',
        host: j['host'] as String? ?? '',
        port: j['port'] as int? ?? 64738,
        username: j['username'] as String? ?? '',
        password: j['password'] as String?,
        certFingerprint: j['certFingerprint'] as String?,
      );

  ServerConfig toConfig() => ServerConfig(
        id: id,
        name: name,
        host: host,
        port: port,
        username: username,
        password: password,
        certFingerprint: certFingerprint,
      );
}

/// Live state for one server connection.
class ServerRuntime {
  ConnStatus status = ConnStatus.idle;
  String detail = '';
  int attempt = 0;
  int retryInMs = 0;
  List<UiUser> users = const [];
  List<UiChannel> channels = const [];
  double tcpPingMs = 0;
  double udpPingMs = 0;
  String transport = 'tcp';
  String? pendingFingerprint;
  bool certificateChanged = false;
  String welcome = '';
  final List<String> messages = [];

  bool get isLive => status == ConnStatus.connected;
  bool get isBusy =>
      status == ConnStatus.connecting ||
      status == ConnStatus.handshaking ||
      status == ConnStatus.reconnecting;
}

/// Central application state.
///
/// Holds the saved server list, the live status of each connection, and the
/// microphone controls. Widgets listen to this rather than the raw Rust stream.
class AppState extends ChangeNotifier {
  static const _prefsKey = 'mumbleway.servers';
  static const _prefsNoise = 'mumbleway.noise';
  static const _prefsMic = 'mumbleway.micMode';

  final List<SavedServer> servers = [];
  final Map<String, ServerRuntime> runtimes = {};

  StreamSubscription<AppEvent>? _events;

  bool _ready = false;
  String? _startupError;
  bool _muted = false;
  bool _deafened = false;
  bool _transmitting = false;
  double _inputLevelDb = -120;
  bool _speaking = false;

  NoiseSetting noise = NoiseSetting.helmet;
  MicMode micMode = MicMode.pushToTalk;
  int maxServers = 2;

  bool get ready => _ready;
  String? get startupError => _startupError;
  bool get muted => _muted;
  bool get deafened => _deafened;
  bool get transmitting => _transmitting;
  double get inputLevelDb => _inputLevelDb;
  bool get speaking => _speaking;

  /// How many servers are currently registered with the engine.
  int get activeCount => runtimes.length;
  bool get canAddMore => runtimes.length < maxServers;

  ServerRuntime runtimeFor(String id) =>
      runtimes.putIfAbsent(id, () => ServerRuntime());

  Future<void> start() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      _loadSettings(prefs);

      final dir = await getApplicationSupportDirectory();
      await startEngine(
        options: StartupOptions(
          storageDir: dir.path,
          noise: noise,
          micMode: micMode,
        ),
      );

      maxServers = maxConcurrentServers();
      _events = appEvents().listen(_onEvent, onError: (Object e) {
        _startupError = e.toString();
        notifyListeners();
      });

      await _loadServers(prefs);
      _ready = true;
    } catch (e) {
      // Most commonly a missing microphone or a denied permission. Surface it
      // rather than leaving the UI stuck on a spinner.
      _startupError = e.toString();
    }
    notifyListeners();
  }

  void _loadSettings(SharedPreferences prefs) {
    final n = prefs.getInt(_prefsNoise);
    if (n != null && n >= 0 && n < NoiseSetting.values.length) {
      noise = NoiseSetting.values[n];
    }
    final m = prefs.getInt(_prefsMic);
    if (m != null && m >= 0 && m < MicMode.values.length) {
      micMode = MicMode.values[m];
    }
  }

  Future<void> _loadServers(SharedPreferences prefs) async {
    final raw = prefs.getStringList(_prefsKey) ?? const [];
    servers
      ..clear()
      ..addAll(raw.map((s) =>
          SavedServer.fromJson(jsonDecode(s) as Map<String, dynamic>)));

    // Register saved servers with the engine, up to the concurrency limit.
    for (final s in servers.take(maxServers)) {
      await _register(s);
    }
  }

  Future<void> _persist() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setStringList(
      _prefsKey,
      servers.map((s) => jsonEncode(s.toJson())).toList(),
    );
    await prefs.setInt(_prefsNoise, noise.index);
    await prefs.setInt(_prefsMic, micMode.index);
  }

  Future<void> _register(SavedServer s) async {
    try {
      await addServer(config: s.toConfig());
      runtimeFor(s.id);
    } catch (e) {
      runtimeFor(s.id)
        ..status = ConnStatus.failed
        ..detail = e.toString();
    }
  }

  /// Adds a server, registers it and saves it.
  Future<String?> addNewServer(SavedServer s) async {
    if (servers.any((e) => e.id == s.id)) {
      return 'That server is already in your list.';
    }
    if (!canAddMore) {
      return 'You can connect to at most $maxServers servers at once.';
    }
    servers.add(s);
    await _register(s);
    await _persist();
    notifyListeners();
    return null;
  }

  Future<void> forgetServer(String id) async {
    servers.removeWhere((s) => s.id == id);
    runtimes.remove(id);
    try {
      await removeServer(serverId: id);
    } catch (_) {
      // Already gone from the engine; the UI state is what matters here.
    }
    await _persist();
    notifyListeners();
  }

  Future<void> connect(String id) async {
    try {
      await connectServer(serverId: id);
    } catch (e) {
      runtimeFor(id)
        ..status = ConnStatus.failed
        ..detail = e.toString();
      notifyListeners();
    }
  }

  Future<void> disconnect(String id) async {
    try {
      await disconnectServer(serverId: id);
    } catch (_) {
      // The session may already be down.
    }
  }

  Future<void> trustChangedCertificate(String id) async {
    final rt = runtimeFor(id);
    final fp = rt.pendingFingerprint;
    if (fp == null) return;

    final i = servers.indexWhere((s) => s.id == id);
    if (i >= 0) {
      servers[i] = servers[i].copyWith(certFingerprint: fp);
      await _persist();
    }
    rt.certificateChanged = false;
    notifyListeners();
    try {
      await acceptCertificate(serverId: id);
    } catch (_) {}
  }

  Future<void> joinChannelOn(String id, int channelId) async {
    try {
      await joinChannel(serverId: id, channelId: channelId);
    } catch (_) {}
  }

  // --- microphone -------------------------------------------------------

  void setTransmit(bool on) {
    if (_transmitting == on) return;
    _transmitting = on;
    setTransmitting(on_: on);
    notifyListeners();
  }

  void toggleMute() {
    _muted = !_muted;
    setMicrophoneMuted(muted: _muted);
    notifyListeners();
  }

  void toggleDeafen() {
    _deafened = !_deafened;
    setDeafened(deafened: _deafened);
    notifyListeners();
  }

  Future<void> updateNoise(NoiseSetting v) async {
    noise = v;
    await _persist();
    notifyListeners();
  }

  Future<void> updateMicMode(MicMode v) async {
    micMode = v;
    // Push-to-talk must not leave a stale open mic behind.
    if (v != MicMode.pushToTalk && _transmitting) {
      setTransmit(false);
    }
    await _persist();
    notifyListeners();
  }

  // --- event handling ---------------------------------------------------

  void _onEvent(AppEvent event) {
    switch (event) {
      case AppEvent_Status(:final field0):
        final rt = runtimeFor(field0.serverId);
        rt
          ..status = field0.status
          ..detail = field0.detail
          ..attempt = field0.attempt
          ..retryInMs = field0.retryInMs.toInt();
      case AppEvent_Users(:final serverId, :final users):
        runtimeFor(serverId).users = users;
      case AppEvent_Channels(:final serverId, :final channels):
        runtimeFor(serverId).channels = channels;
      case AppEvent_Text(:final serverId, :final from, :final message):
        final rt = runtimeFor(serverId);
        rt.messages.add('$from: $message');
        // Keep the log bounded; this can run for hours.
        if (rt.messages.length > 200) rt.messages.removeAt(0);
      case AppEvent_Stats(:final field0):
        final rt = runtimeFor(field0.serverId);
        rt
          ..tcpPingMs = field0.tcpPingMs
          ..udpPingMs = field0.udpPingMs
          ..transport = field0.transport;
      case AppEvent_InputLevel(:final levelDb, :final speaking):
        _inputLevelDb = levelDb;
        _speaking = speaking;
      case AppEvent_Certificate(
          :final serverId,
          :final fingerprint,
          :final changed
        ):
        final rt = runtimeFor(serverId);
        rt
          ..pendingFingerprint = fingerprint
          ..certificateChanged = changed;
        if (!changed) {
          // First contact: remember the fingerprint so a later change is caught.
          final i = servers.indexWhere((s) => s.id == serverId);
          if (i >= 0 && servers[i].certFingerprint == null) {
            servers[i] = servers[i].copyWith(certFingerprint: fingerprint);
            unawaited(_persist());
          }
        }
      case AppEvent_Welcome(:final serverId, :final text):
        runtimeFor(serverId).welcome = text;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _events?.cancel();
    super.dispose();
  }
}

/// Makes [AppState] available to the widget tree.
class AppStateScope extends InheritedNotifier<AppState> {
  const AppStateScope({super.key, required AppState state, required super.child})
      : super(notifier: state);

  static AppState of(BuildContext context) {
    final scope =
        context.dependOnInheritedWidgetOfExactType<AppStateScope>();
    assert(scope != null, 'No AppStateScope found in context');
    return scope!.notifier!;
  }
}
