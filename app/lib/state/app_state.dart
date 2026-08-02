import 'dart:async';
import 'dart:convert';
import 'dart:io' show File, Platform;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/widgets.dart';
import 'package:http/http.dart' as http;
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../services/overlay.dart';
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
    this.defaultChannel,
    String? localId,
  }) : localId = localId ?? '$host:$port';

  final String name;
  final String host;
  final int port;
  final String username;
  final String? password;
  final String? certFingerprint;

  /// Channel joined automatically on every connect.
  final String? defaultChannel;

  /// Unique key for this entry.
  ///
  /// Defaults to `host:port`, which is what the Rust core derives, but is
  /// stored explicitly so the same server can be kept more than once — under a
  /// different username or channel, say. Duplicates get a suffix.
  final String localId;

  String get id => localId;

  SavedServer copyWith({
    String? name,
    String? username,
    String? certFingerprint,
    String? defaultChannel,
    String? localId,
    bool clearDefaultChannel = false,
  }) =>
      SavedServer(
        name: name ?? this.name,
        host: host,
        port: port,
        username: username ?? this.username,
        password: password,
        certFingerprint: certFingerprint ?? this.certFingerprint,
        defaultChannel:
            clearDefaultChannel ? null : (defaultChannel ?? this.defaultChannel),
        localId: localId ?? this.localId,
      );

  Map<String, dynamic> toJson() => {
        'localId': localId,
        'name': name,
        'host': host,
        'port': port,
        'username': username,
        'password': password,
        'certFingerprint': certFingerprint,
        'defaultChannel': defaultChannel,
      };

  static SavedServer fromJson(Map<String, dynamic> j) => SavedServer(
        // Entries saved before duplicates existed have no localId; falling back
        // to host:port keeps them working and matches their old key exactly.
        localId: j['localId'] as String?,
        name: j['name'] as String? ?? '',
        host: j['host'] as String? ?? '',
        port: j['port'] as int? ?? 64738,
        username: j['username'] as String? ?? '',
        password: j['password'] as String?,
        certFingerprint: j['certFingerprint'] as String?,
        defaultChannel: j['defaultChannel'] as String?,
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

/// An entry from the public server directory.
class PublicServer {
  const PublicServer({
    required this.name,
    required this.host,
    required this.port,
    this.country = '',
  });

  final String name;
  final String host;
  final int port;
  final String country;
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

  /// Our own session id, once the server assigns it.
  int? selfSession;

  /// Result of the most recent unauthenticated status probe.
  UiServerStatus? probe;

  bool get isLive => status == ConnStatus.connected;
  bool get isBusy =>
      status == ConnStatus.connecting ||
      status == ConnStatus.handshaking ||
      status == ConnStatus.reconnecting;

  /// The channel we are currently in, if known.
  int? get currentChannelId {
    final me = selfSession;
    if (me == null) return null;
    for (final u in users) {
      if (u.session == me) return u.channelId;
    }
    return null;
  }

  UiChannel? get currentChannel {
    final id = currentChannelId;
    if (id == null) return null;
    for (final c in channels) {
      if (c.id == id) return c;
    }
    return null;
  }

  /// Everyone in our channel except us.
  List<UiUser> get channelPeers {
    final id = currentChannelId;
    if (id == null) return const [];
    return users
        .where((u) => u.channelId == id && u.session != selfSession)
        .toList();
  }

  /// Names of everyone currently talking, for the floating island.
  List<String> get speakingNames =>
      users.where((u) => u.talking).map((u) => u.name).toList();
}

/// Central application state.
class AppState extends ChangeNotifier {
  static const _prefsKey = 'mumbleway.servers';
  static const _prefsNoise = 'mumbleway.noise';
  static const _prefsMic = 'mumbleway.micMode';
  static const _prefsInputDevice = 'mumbleway.inputDevice';
  static const _prefsOutputDevice = 'mumbleway.outputDevice';
  static const _prefsInputGain = 'mumbleway.inputGain';
  static const _prefsOutputVolume = 'mumbleway.outputVolume';

  /// How often saved servers are re-probed for ping and occupancy.
  static const _pingInterval = Duration(seconds: 15);

  final List<SavedServer> servers = [];
  final Map<String, ServerRuntime> runtimes = {};

  StreamSubscription<AppEvent>? _events;
  Timer? _pingTimer;

  bool _ready = false;
  String? _startupError;
  bool _muted = false;
  bool _deafened = false;
  bool _transmitting = false;
  double _inputLevelDb = -120;
  double _thresholdDb = -120;
  bool _speaking = false;

  /// Most recent moderation applied to us by someone else, for a banner.
  String? lastModerationMessage;

  NoiseSetting noise = NoiseSetting.helmet;
  MicMode micMode = MicMode.pushToTalk;
  int maxServers = 2;

  // --- audio devices ----------------------------------------------------
  List<String> inputDevices = const [];
  List<String> outputDevices = const [];
  String? selectedInput;
  String? selectedOutput;
  double inputGainDbValue = 0;
  double outputVolumeDbValue = 0;
  bool monitoring = false;

  /// `[minInputGain, maxInputGain, minOutputVolume, maxOutputVolume]`.
  List<double> gainRange = const [-20, 30, -40, 10];

  bool get ready => _ready;
  String? get startupError => _startupError;
  bool get muted => _muted;
  bool get deafened => _deafened;
  bool get transmitting => _transmitting;
  double get inputLevelDb => _inputLevelDb;

  /// Level voice activation opens at. Tracks the background noise, so it rises
  /// with engine and wind — which is what makes it worth showing.
  double get activationThresholdDb => _thresholdDb;
  bool get speaking => _speaking;

  /// Whether the talk button is relevant. In the automatic modes it is not,
  /// and the vertical space is better spent on the server list.
  bool get showTalkButton => micMode == MicMode.pushToTalk;

  int get activeCount => runtimes.length;
  bool get canAddMore => runtimes.length < maxServers;

  ServerRuntime runtimeFor(String id) =>
      runtimes.putIfAbsent(id, () => ServerRuntime());

  /// Every user currently talking across all connected servers.
  List<String> get allSpeakingNames => [
        for (final rt in runtimes.values)
          if (rt.isLive) ...rt.speakingNames,
      ];

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
      gainRange = gainLimits();

      _events = appEvents().listen(_onEvent, onError: (Object e) {
        _startupError = e.toString();
        notifyListeners();
      });

      await _applyAudioSettings();
      await refreshDevices();
      await _loadServers(prefs);

      _pingTimer = Timer.periodic(_pingInterval, (_) => refreshPings());
      unawaited(refreshPings());

      _ready = true;
    } catch (e) {
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
    selectedInput = prefs.getString(_prefsInputDevice);
    selectedOutput = prefs.getString(_prefsOutputDevice);
    inputGainDbValue = prefs.getDouble(_prefsInputGain) ?? 0;
    outputVolumeDbValue = prefs.getDouble(_prefsOutputVolume) ?? 0;
  }

  Future<void> _applyAudioSettings() async {
    setInputGainDb(db: inputGainDbValue);
    setOutputVolumeDb(db: outputVolumeDbValue);
    if (selectedInput != null || selectedOutput != null) {
      await setAudioDevices(input: selectedInput, output: selectedOutput);
    }
  }

  Future<void> _loadServers(SharedPreferences prefs) async {
    final raw = prefs.getStringList(_prefsKey) ?? const [];
    servers
      ..clear()
      ..addAll(raw.map((s) =>
          SavedServer.fromJson(jsonDecode(s) as Map<String, dynamic>)));

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
    await prefs.setDouble(_prefsInputGain, inputGainDbValue);
    await prefs.setDouble(_prefsOutputVolume, outputVolumeDbValue);
    if (selectedInput == null) {
      await prefs.remove(_prefsInputDevice);
    } else {
      await prefs.setString(_prefsInputDevice, selectedInput!);
    }
    if (selectedOutput == null) {
      await prefs.remove(_prefsOutputDevice);
    } else {
      await prefs.setString(_prefsOutputDevice, selectedOutput!);
    }
  }

  Future<void> _register(SavedServer s) async {
    try {
      await addServer(config: s.toConfig());
      runtimeFor(s.id);
      if (s.defaultChannel != null) {
        await setDefaultChannel(serverId: s.id, channel: s.defaultChannel);
      }
    } catch (e) {
      runtimeFor(s.id)
        ..status = ConnStatus.failed
        ..detail = e.toString();
    }
  }

  // --- servers ----------------------------------------------------------

  Future<String?> addNewServer(SavedServer s) async {
    if (servers.any((e) => e.id == s.id)) {
      return 'That server is already in your list.';
    }
    servers.add(s);
    // Only the first `maxServers` get a live session; the rest stay as saved
    // entries the user can swap in.
    if (runtimes.length < maxServers) {
      await _register(s);
    }
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  /// Imports servers from a `mumble://` link or JSON profile text.
  Future<String?> importFromText(String text, {String? fallbackUsername}) async {
    try {
      final configs = await importServers(
        text: text,
        fallbackUsername: fallbackUsername ?? _suggestUsername(),
      );
      var added = 0;
      for (final c in configs) {
        final s = SavedServer(
          name: c.name,
          host: c.host,
          port: c.port,
          username: c.username,
          password: c.password,
          certFingerprint: c.certFingerprint,
        );
        if (servers.any((e) => e.id == s.id)) continue;
        servers.add(s);
        if (runtimes.length < maxServers) await _register(s);
        added++;
      }
      await _persist();
      notifyListeners();
      unawaited(refreshPings());
      if (added == 0) return 'Those servers are already in your list.';
      return null;
    } catch (e) {
      return e.toString().replaceFirst('Exception: ', '');
    }
  }

  /// Downloads a profile file and imports whatever it contains.
  Future<String?> importFromUrl(String url) async {
    try {
      final uri = Uri.parse(url.trim());
      final res = await http.get(uri).timeout(const Duration(seconds: 20));
      if (res.statusCode != 200) {
        return 'Download failed (HTTP ${res.statusCode}).';
      }
      return importFromText(res.body);
    } catch (e) {
      return 'Could not download that file: $e';
    }
  }

  String _suggestUsername() =>
      servers.isNotEmpty ? servers.first.username : 'rider';

  /// Picks an unused local id for a new entry.
  String _uniqueId(String host, int port) {
    final base = '$host:$port';
    if (!servers.any((s) => s.localId == base)) return base;
    var n = 2;
    while (servers.any((s) => s.localId == '$base#$n')) {
      n++;
    }
    return '$base#$n';
  }

  /// Copies a saved server, so the same host can be kept under a different
  /// username or default channel.
  Future<String?> duplicateServer(SavedServer s) async {
    final copy = s.copyWith(
      name: '${s.name} (copy)',
      localId: _uniqueId(s.host, s.port),
    );
    servers.add(copy);
    if (runtimes.length < maxServers) await _register(copy);
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  // --- export and import --------------------------------------------------

  /// Writes every saved server to a JSON file.
  ///
  /// Desktop gets a save dialog; mobile has no user-visible filesystem, so the
  /// file goes to a temporary path and straight into the share sheet.
  Future<String?> exportServersToFile() async {
    if (servers.isEmpty) return 'There are no servers to export.';
    try {
      final json = await exportServers(
        configs: servers.map((s) => s.toConfig()).toList(),
      );
      const fileName = 'mumbleway-servers.json';

      if (Platform.isAndroid || Platform.isIOS) {
        final dir = await getTemporaryDirectory();
        final file = File('${dir.path}/$fileName');
        await file.writeAsString(json);
        await SharePlus.instance.share(
          ShareParams(files: [XFile(file.path)], subject: 'MumbleWay servers'),
        );
        return null;
      }

      final location = await getSaveLocation(
        suggestedName: fileName,
        acceptedTypeGroups: const [
          XTypeGroup(label: 'JSON', extensions: ['json'])
        ],
      );
      if (location == null) return null; // cancelled
      await File(location.path).writeAsString(json);
      return null;
    } catch (e) {
      return 'Export failed: $e';
    }
  }

  /// Reads a profile file chosen by the user and adds what it contains.
  Future<String?> importServersFromFile() async {
    try {
      final file = await openFile(
        acceptedTypeGroups: const [
          XTypeGroup(label: 'Server profiles', extensions: ['json', 'mumble'])
        ],
      );
      if (file == null) return null; // cancelled
      final text = await file.readAsString();
      return importFromText(text);
    } catch (e) {
      return 'Import failed: $e';
    }
  }

  // --- sharing ------------------------------------------------------------

  /// Shares an invite for [s] as a link, optionally with the password baked in.
  Future<String?> shareInviteLink(
    SavedServer s, {
    String? channel,
    bool includePassword = false,
  }) async {
    try {
      final link = await buildInviteLink(
        config: s.toConfig(),
        channel: channel,
        includePassword: includePassword,
      );
      await SharePlus.instance.share(
        ShareParams(text: link, subject: 'Join me on ${s.name}'),
      );
      return null;
    } catch (e) {
      return 'Could not share: $e';
    }
  }

  /// Shares an invite for [s] as a profile file.
  Future<String?> shareInviteFile(
    SavedServer s, {
    String? channel,
    bool includePassword = false,
  }) async {
    try {
      final json = await buildInviteFile(
        config: s.toConfig(),
        channel: channel,
        includePassword: includePassword,
      );
      final dir = await getTemporaryDirectory();
      final safe = s.name.replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '_');
      final file = File('${dir.path}/$safe.json');
      await file.writeAsString(json);
      await SharePlus.instance.share(
        ShareParams(files: [XFile(file.path)], subject: 'Join me on ${s.name}'),
      );
      return null;
    } catch (e) {
      return 'Could not share: $e';
    }
  }

  Future<void> forgetServer(String id) async {
    servers.removeWhere((s) => s.id == id);
    runtimes.remove(id);
    try {
      await removeServer(serverId: id);
    } catch (_) {}
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
    } catch (_) {}
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

  // --- ping -------------------------------------------------------------

  /// Re-probes every saved server. Offline servers simply report unreachable.
  Future<void> refreshPings() async {
    final snapshot = List<SavedServer>.from(servers);
    await Future.wait(snapshot.map((s) async {
      try {
        final status =
            await pingServer(serverId: s.id, host: s.host, port: s.port);
        runtimeFor(s.id).probe = status;
      } catch (_) {
        // Probing is best-effort; a failure just leaves the previous reading.
      }
    }));
    notifyListeners();
  }

  // --- channels and users ------------------------------------------------

  Future<void> joinChannelOn(String id, int channelId) async {
    try {
      await joinChannel(serverId: id, channelId: channelId);
    } catch (_) {}
  }

  /// Sets (or clears) the channel joined automatically on connect.
  Future<void> setDefaultChannelFor(String id, String? channelName) async {
    final i = servers.indexWhere((s) => s.id == id);
    if (i >= 0) {
      servers[i] = servers[i].copyWith(
        defaultChannel: channelName,
        clearDefaultChannel: channelName == null,
      );
      await _persist();
    }
    try {
      await setDefaultChannel(serverId: id, channel: channelName);
    } catch (_) {}
    notifyListeners();
  }

  Future<void> toggleUserLocalMute(String id, UiUser user) async {
    try {
      await setUserLocalMute(
        serverId: id,
        session: user.session,
        muted: !user.localMute,
      );
    } catch (_) {}
  }

  Future<void> toggleUserServerMute(String id, UiUser user) async {
    try {
      await setUserServerMute(
        serverId: id,
        session: user.session,
        muted: !user.muted,
      );
    } catch (_) {}
  }

  Future<void> toggleUserServerDeaf(String id, UiUser user) async {
    try {
      await setUserServerDeaf(
        serverId: id,
        session: user.session,
        deaf: !user.deafened,
      );
    } catch (_) {}
  }

  /// Removes a user from the server. Requires the Kick permission; the server
  /// replies with a permission-denied message if we lack it, which arrives in
  /// the message log rather than as a thrown error.
  Future<String?> kickUserFrom(String id, UiUser user, String reason) async {
    try {
      await kickUser(serverId: id, session: user.session, reason: reason);
      return null;
    } catch (e) {
      return e.toString();
    }
  }

  // --- audio -------------------------------------------------------------

  Future<void> refreshDevices() async {
    try {
      inputDevices = await audioInputDevices();
      outputDevices = await audioOutputDevices();
      notifyListeners();
    } catch (_) {}
  }

  Future<void> chooseInputDevice(String? name) async {
    selectedInput = name;
    await setAudioDevices(input: selectedInput, output: selectedOutput);
    await _persist();
    notifyListeners();
  }

  Future<void> chooseOutputDevice(String? name) async {
    selectedOutput = name;
    await setAudioDevices(input: selectedInput, output: selectedOutput);
    await _persist();
    notifyListeners();
  }

  Future<void> updateInputGain(double db) async {
    inputGainDbValue = db;
    setInputGainDb(db: db);
    notifyListeners();
    await _persist();
  }

  Future<void> updateOutputVolume(double db) async {
    outputVolumeDbValue = db;
    setOutputVolumeDb(db: db);
    notifyListeners();
    await _persist();
  }

  void toggleMonitoring() {
    monitoring = !monitoring;
    setMonitoring(on_: monitoring);
    notifyListeners();
  }

  void testOutput() => playTestTone(millis: 700);

  void setTransmit(bool on) {
    if (_transmitting == on) return;
    _transmitting = on;
    setTransmitting(on_: on);
    notifyListeners();
    _pushOverlay();
  }

  // --- floating island ---------------------------------------------------

  final OverlayBridge overlay = OverlayBridge.instance;
  bool overlayEnabled = false;
  String _lastOverlaySignature = '';

  bool get overlaySupported => overlay.isSupported;

  /// Shows the floating island. Returns an error message, or null on success.
  Future<String?> enableOverlay() async {
    if (!overlay.isSupported) {
      return 'Floating overlays are not available on this platform.';
    }
    if (!await overlay.hasPermission()) {
      // Android only grants this from its own settings screen.
      await overlay.requestPermission();
      if (!await overlay.hasPermission()) {
        return 'Allow "display over other apps" for MumbleWay, then try again.';
      }
    }

    // The island is another view onto the same transmit state, not a second
    // source of truth: its button routes back through here.
    overlay.onTransmit = setTransmit;

    final error = await overlay.show();
    overlayEnabled = error == null;
    notifyListeners();
    _lastOverlaySignature = '';
    _pushOverlay();
    return error;
  }

  Future<void> disableOverlay() async {
    await overlay.hide();
    overlayEnabled = false;
    notifyListeners();
  }

  /// Pushes speakers and transmit state onto the island, skipping the call
  /// when nothing visible has changed — this runs on every roster update.
  void _pushOverlay() {
    if (!overlayEnabled) return;
    final names = allSpeakingNames;
    final connected = runtimes.values.any((r) => r.isLive);
    final signature = '${names.join(',')}|$_transmitting|$connected';
    if (signature == _lastOverlaySignature) return;
    _lastOverlaySignature = signature;
    unawaited(overlay.update(
      names: names,
      transmitting: _transmitting,
      connected: connected,
    ));
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
    if (v != MicMode.pushToTalk && _transmitting) setTransmit(false);
    await _persist();
    notifyListeners();
  }

  // --- public server directory -------------------------------------------

  /// Fetches the public server directory (around 260 servers).
  ///
  /// Two things this request must get right, both learned the hard way:
  ///
  /// * **`Accept-Encoding: gzip` is mandatory.** The endpoint answers
  ///   `501 Not Implemented` with an empty body to any client that does not
  ///   advertise gzip — which looks exactly like the service being down. Dart's
  ///   `HttpClient` sends it by default, and it is requested explicitly here so
  ///   that stays true if the client is ever swapped out.
  /// * **`version` is required** — without it the endpoint also returns 501.
  ///
  /// [usedFallback] reports whether the network request failed and a small
  /// built-in list was substituted.
  Future<(List<PublicServer>, bool usedFallback)> fetchPublicServers() async {
    try {
      final uri =
          Uri.parse('https://publist.mumble.info/v1/list?version=1.5.735');
      final res = await http.get(uri, headers: {
        'Accept-Encoding': 'gzip',
        'Accept': '*/*',
      }).timeout(const Duration(seconds: 20));

      if (res.statusCode == 200 && res.body.contains('<server')) {
        final parsed = _parsePublicList(res.body);
        if (parsed.isNotEmpty) return (parsed, false);
      }
    } catch (_) {
      // Network trouble; fall through to the built-in list.
    }
    return (_knownPublicServers, true);
  }

  /// Parses the directory's XML without pulling in an XML package: the schema
  /// is a flat list of self-closing `<server .../>` elements.
  static List<PublicServer> _parsePublicList(String xml) {
    final out = <PublicServer>[];
    final re = RegExp(r'<server\b([^>]*)/?>', caseSensitive: false);
    final attr = RegExp('([a-zA-Z_]+)\\s*=\\s*"([^"]*)"');

    for (final m in re.allMatches(xml)) {
      final attrs = <String, String>{};
      for (final a in attr.allMatches(m.group(1) ?? '')) {
        attrs[a.group(1)!.toLowerCase()] = a.group(2)!;
      }
      final host = attrs['ip'] ?? '';
      final port = int.tryParse(attrs['port'] ?? '') ?? 64738;
      if (host.isEmpty) continue;
      out.add(PublicServer(
        name: attrs['name'] ?? host,
        host: host,
        port: port,
        country: attrs['country'] ?? '',
      ));
    }
    return out;
  }

  static const List<PublicServer> _knownPublicServers = [
    PublicServer(
        name: 'Mumble.info (official test)', host: 'mumble.info', port: 64738),
    PublicServer(name: 'GetMumble EU', host: 'eu.getmumble.com', port: 64738),
    PublicServer(name: 'GetMumble US', host: 'us.getmumble.com', port: 64738),
  ];

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
      case AppEvent_SelfSession(:final serverId, :final session):
        runtimeFor(serverId).selfSession = session;
      case AppEvent_Text(:final serverId, :final from, :final message):
        final rt = runtimeFor(serverId);
        rt.messages.add('$from: $message');
        if (rt.messages.length > 200) rt.messages.removeAt(0);
      case AppEvent_Stats(:final field0):
        final rt = runtimeFor(field0.serverId);
        rt
          ..tcpPingMs = field0.tcpPingMs
          ..udpPingMs = field0.udpPingMs
          ..transport = field0.transport;
      case AppEvent_InputLevel(
          :final levelDb,
          :final speaking,
          :final thresholdDb
        ):
        _inputLevelDb = levelDb;
        _speaking = speaking;
        _thresholdDb = thresholdDb;
      case AppEvent_Moderated(
          :final muted,
          :final deafened,
          :final by,
        ):
        // The cue already played in the core; this is the visible half.
        final what = deafened != null
            ? (deafened ? 'deafened you' : 'undeafened you')
            : (muted == true ? 'muted you' : 'unmuted you');
        lastModerationMessage = '$by $what';
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
    // Keep the island's speaker list in step with the roster.
    _pushOverlay();
  }

  @override
  void dispose() {
    _pingTimer?.cancel();
    _events?.cancel();
    super.dispose();
  }
}

/// Makes [AppState] available to the widget tree.
class AppStateScope extends InheritedNotifier<AppState> {
  const AppStateScope({super.key, required AppState state, required super.child})
      : super(notifier: state);

  static AppState of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<AppStateScope>();
    assert(scope != null, 'No AppStateScope found in context');
    return scope!.notifier!;
  }
}
