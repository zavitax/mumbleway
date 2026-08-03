import 'dart:async';
import 'dart:convert';
import 'dart:io' show File, Platform;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../l10n/app_localizations.dart';
import '../services/audio_session.dart';
import '../services/button_controller.dart';
import '../services/cloud_sync.dart';
import '../services/engine_log.dart';
import '../services/overlay.dart';
import '../services/proxy.dart';
import '../src/rust/api/mumbleway.dart';
import 'server_sync.dart';
import '../widgets/voice_meter.dart';

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
    this.updatedAt = 0,
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
  /// stored explicitly so the same server can be kept more than once вЂ” under a
  /// different username or channel, say. Duplicates get a suffix.
  final String localId;

  /// When this entry was last edited, in milliseconds since the epoch.
  ///
  /// Only sync reads it, to settle which of two devices' versions of the same
  /// entry is the current one. Entries saved before sync existed carry 0 and
  /// so lose every such contest, which is right: anything with a real
  /// timestamp has been touched since, and this one has not.
  final int updatedAt;

  String get id => localId;

  /// A copy marked as changed just now.
  ///
  /// Called at the few points where the user actually alters the list, rather
  /// than inside [copyWith], which is also used for edits that are nobody
  /// else's business вЂ” and stamping those would have this device win conflicts
  /// it took no part in.
  SavedServer stamped() =>
      copyWith(updatedAt: DateTime.now().millisecondsSinceEpoch);

  /// Whether two versions differ in anything the live session is built from.
  ///
  /// A renamed server does not need its connection torn down and rebuilt; a
  /// re-hosted one does.
  bool sameConnection(SavedServer o) =>
      host == o.host &&
      port == o.port &&
      username == o.username &&
      password == o.password &&
      certFingerprint == o.certFingerprint &&
      defaultChannel == o.defaultChannel;

  SavedServer copyWith({
    String? name,
    String? username,
    String? certFingerprint,
    String? defaultChannel,
    String? localId,
    int? updatedAt,
    bool clearDefaultChannel = false,
  }) => SavedServer(
    updatedAt: updatedAt ?? this.updatedAt,
    name: name ?? this.name,
    host: host,
    port: port,
    username: username ?? this.username,
    password: password,
    certFingerprint: certFingerprint ?? this.certFingerprint,
    defaultChannel: clearDefaultChannel
        ? null
        : (defaultChannel ?? this.defaultChannel),
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
    'updatedAt': updatedAt,
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
    updatedAt: (j['updatedAt'] as num?)?.toInt() ?? 0,
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

  /// When the next attempt is due, so the notice can count down in real time.
  DateTime? retryDeadline;

  /// Whole seconds left before the next attempt, floored at zero.
  int get retrySecondsLeft {
    final deadline = retryDeadline;
    if (deadline == null) return 0;
    final left = deadline.difference(DateTime.now()).inMilliseconds;
    return left <= 0 ? 0 : (left / 1000).ceil();
  }

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

  /// Whether the saved entry behind this session may be edited or removed.
  ///
  /// Only when genuinely disconnected: never started, stopped, or given up.
  /// Anything else has a session either running or trying to, and both editing
  /// and removing pull that session out from under itself.
  bool get isModifiable => !isLive && !isBusy;

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

  /// Current loudness per speaker session, in dBFS.
  ///
  /// Comes from the decoded audio: the server never says who is talking, so
  /// the roster cannot know it.
  final Map<int, double> speakerLevels = {};

  /// Sessions seen in our own channel at the last roster update.
  ///
  /// Null until the first roster for a given channel arrives: joining a busy
  /// channel would otherwise announce everybody already sitting in it, which
  /// is noise rather than news.
  Set<int>? knownPeers;

  /// When a join or leave was last announced, so a roster that flaps cannot
  /// turn into a repeating tone.
  DateTime? lastCueAt;

  /// The channel [knownPeers] was collected for, so moving channels starts a
  /// fresh comparison rather than reporting the whole new room as arrivals.
  int? knownPeersChannel;

  /// Names of everyone currently talking, for the floating island.
  /// Above this a speaker counts as talking, and the meter lights up.
  static const speakingFloorDb = -55.0;

  bool isSpeaking(int session) =>
      (speakerLevels[session] ?? _silentDb) > speakingFloorDb;

  static const _silentDb = VoiceMeter.silentDb;

  /// Records a level, rising at once and falling no faster than the limit.
  void noteSpeakerLevel(int session, double levelDb) {
    speakerLevels[session] = VoiceMeter.follow(
      speakerLevels[session] ?? _silentDb,
      levelDb,
    );
  }

  /// Lets everyone absent from a report fall towards silence.
  void decayUnreported(Set<int> reported) {
    for (final session in speakerLevels.keys.toList()) {
      if (reported.contains(session)) continue;
      final current = speakerLevels[session]!;
      if (current <= _silentDb) {
        speakerLevels.remove(session);
      } else {
        speakerLevels[session] = VoiceMeter.follow(current, _silentDb);
      }
    }
  }

  List<String> get speakingNames =>
      users.where((u) => isSpeaking(u.session)).map((u) => u.name).toList();
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
  static const _prefsEchoCancellation = 'mumbleway.echoCancellation';
  static const _prefsNormaliseLevels = 'mumbleway.normaliseLevels';
  static const _prefsReverb = 'mumbleway.reverb';
  static const _prefsFeedbackGuard = 'mumbleway.feedbackGuard';
  static const _prefsSettingStamps = 'mumbleway.settingStamps';
  static const _prefsProxyEnabled = 'mumbleway.proxyEnabled';
  static const _prefsProxyManual = 'mumbleway.proxyManual';
  static const _prefsLocale = 'mumbleway.locale';
  static const _prefsButtons = 'mumbleway.buttonBindings';
  static const _prefsCloudSync = 'mumbleway.cloudSync';
  static const _prefsFloatingWindow = 'mumbleway.floatingWindow';
  static const _prefsDeleted = 'mumbleway.deletedServers';

  /// Languages the interface is available in.
  static const supportedLocales = [Locale('en'), Locale('ru')];

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
  double _noiseFloorDb = -120;
  bool _speaking = false;

  /// Most recent moderation applied to us by someone else, for a banner.
  String? lastModerationMessage;

  /// Server shown in the detail pane on wide layouts. Narrow layouts ignore it
  /// and expand cards inline instead.
  String? _selectedServerId;

  String? get selectedServerId {
    // Fall back to something sensible rather than an empty pane: whatever is
    // connected, else the first saved server.
    if (_selectedServerId != null &&
        servers.any((s) => s.id == _selectedServerId)) {
      return _selectedServerId;
    }
    for (final s in servers) {
      if (runtimeFor(s.id).isLive) return s.id;
    }
    return servers.isEmpty ? null : servers.first.id;
  }

  SavedServer? get selectedServer {
    final id = selectedServerId;
    if (id == null) return null;
    for (final s in servers) {
      if (s.id == id) return s;
    }
    return null;
  }

  void selectServer(String id) {
    _selectedServerId = id;
    notifyListeners();
  }

  NoiseSetting noise = NoiseSetting.helmet;
  MicMode micMode = MicMode.pushToTalk;
  int maxServers = 2;

  /// Chosen interface language. Null follows the system.
  Locale? _locale;
  Locale? get locale => _locale;

  /// The app's strings, without a widget to ask.
  ///
  /// The floating window draws its own text and lives outside the widget tree
  /// entirely, so the phrases have to be looked up from the locale rather than
  /// from a context. Falls back to the first supported language when the
  /// system is set to one this app does not speak.
  L get _strings {
    final code = (_locale ?? WidgetsBinding.instance.platformDispatcher.locale)
        .languageCode;
    final known = supportedLocales.any((l) => l.languageCode == code);
    return lookupL(Locale(known ? code : supportedLocales.first.languageCode));
  }

  /// Every phrase the floating window paints for itself.
  Map<String, String> _overlayPhrases() {
    final l = _strings;
    return {
      'pipOnAir': l.pipOnAir,
      'pipTalking': l.pipTalking,
      'pipDeafened': l.pipDeafened,
      'pipMuted': l.pipMuted,
      'pipListening': l.pipListening,
      'pipBadgeMuted': l.pipBadgeMuted,
      'pipBadgeDeafened': l.pipBadgeDeafened,
      'pipNoise': l.pipNoise,
      'pipOpen': l.pipOpen,
      'pipTalk': l.pipTalk,
      'pipHandsFreeVoice': l.pipHandsFreeVoice,
      'pipHandsFreeAlways': l.pipHandsFreeAlways,
      'pipSpeaking': l.pipSpeaking,
      'pipNobodySpeaks': l.pipNobodySpeaks,
      'pipNotConnected': l.pipNotConnected,
    };
  }

  /// Cycles to the next available language. Bound to the flag in the title bar,
  /// which is a one-tap toggle rather than a menu because there are only two.
  Future<void> cycleLocale() async {
    final current =
        _locale?.languageCode ?? supportedLocales.first.languageCode;
    final index = supportedLocales.indexWhere((l) => l.languageCode == current);
    _locale = supportedLocales[(index + 1) % supportedLocales.length];
    notifyListeners();
    // The window keeps its own copy of the wording, so it has to be told.
    unawaited(overlay.setPhrases(_overlayPhrases()));
    _lastOverlaySignature = '';
    _pushOverlay();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_prefsLocale, _locale!.languageCode);
  }

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
  /// with engine and wind вЂ” which is what makes it worth showing.
  double get activationThresholdDb => _thresholdDb;

  /// Tracked background noise. The gap up to [activationThresholdDb] is the
  /// margin voice activation needs to clear.
  double get noiseFloorDb => _noiseFloorDb;
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

      // Before the engine, not after. iOS hands out a playback-only session
      // until asked otherwise, and an engine started against one finds zero
      // input channels and fails inside CoreAudio with wording that describes
      // the symptom and not the cause.
      final session = await AudioSessionBridge.instance.prepare();
      if (!session.granted) {
        _startupError =
            'MumbleWay needs permission to use the microphone. '
            'Allow it in Settings, then reopen the app.';
        notifyListeners();
        return;
      }
      if (session.inputChannels == 0) {
        _startupError =
            session.error ??
            'This device is not offering any audio input right now. '
                'If a headset is connected, try reconnecting it.';
        notifyListeners();
        return;
      }
      // A call or a Siri request takes the session away and does not give it
      // back. The platform side reactivates it; the meters need to know the
      // levels they are showing went stale in the meantime.
      AudioSessionBridge.instance.onResumed = () {
        for (final rt in runtimes.values) {
          rt.speakerLevels.clear();
        }
        notifyListeners();
      };

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

      _events = appEvents().listen(
        _onEvent,
        onError: (Object e) {
          _startupError = e.toString();
          notifyListeners();
        },
      );

      // Resolve the proxy once at startup; createClient() uses the cached
      // result, so no request pays for a subprocess.
      await SystemProxy.instance.refresh();

      _setUpButtons(prefs);

      await _applyAudioSettings();
      await refreshDevices();
      await _loadServers(prefs);

      // After the local list is up, so the first merge has something to merge
      // against, and not awaited, so a slow or absent iCloud cannot hold up
      // startup. The app is usable from its own copy either way.
      CloudSync.instance.onRemoteChange = () => unawaited(syncNow());
      unawaited(syncNow());

      // Not awaited: the window is worth having but nothing else waits on it,
      // and on Android it can fail for want of a permission the user has to
      // grant in system settings. A failure there leaves the setting off
      // without an alarm rather than blocking a startup that is otherwise fine.
      if (_wantOverlay && overlay.isSupported) {
        unawaited(enableOverlay());
      }

      _pingTimer = Timer.periodic(_pingInterval, (_) => refreshPings());
      unawaited(refreshPings());

      _ready = true;
    } catch (e) {
      _startupError = e.toString();
    } finally {
      // Collect what the engine said while starting, on every path out of here.
      //
      // In a `finally` rather than after a successful start, because startup is
      // the case this log exists for and the one where it was unreachable: an
      // engine that fails to start emits the lines explaining why and then
      // throws, and fetching them only on the happy path threw away the
      // evidence at precisely the moment it mattered. The fetch does not need
      // a running engine, which is what makes this work.
      EngineLog.instance.backfill();
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

    // Proxy use defaults to on: a machine behind one usually cannot reach
    // anything without it, and detection reports "direct" when there is none.
    SystemProxy.instance.enabled = prefs.getBool(_prefsProxyEnabled) ?? true;
    // Read here, pushed to the engine in _applyAudioSettings. This runs before
    // the engine is started, and anything that reaches across before then
    // throws.
    echoCancellation = prefs.getBool(_prefsEchoCancellation) ?? true;
    normaliseLevels = prefs.getBool(_prefsNormaliseLevels) ?? true;
    reverb = prefs.getBool(_prefsReverb) ?? true;
    final guard = prefs.getInt(_prefsFeedbackGuard);
    if (guard != null &&
        guard >= 0 &&
        guard < FeedbackGuardMode.values.length) {
      feedbackGuard = FeedbackGuardMode.values[guard];
    }
    // On by default: a user with two devices almost always wants the same
    // servers on both, and there is nothing to configure for it to work.
    cloudSync = prefs.getBool(_prefsCloudSync) ?? true;
    _wantOverlay = prefs.getBool(_prefsFloatingWindow) ?? true;
    SystemProxy.instance.manualProxy = prefs.getString(_prefsProxyManual);

    final code = prefs.getString(_prefsLocale);
    if (code != null && supportedLocales.any((l) => l.languageCode == code)) {
      _locale = Locale(code);
    }
  }

  Future<void> _applyAudioSettings() async {
    setInputGainDb(db: inputGainDbValue);
    setOutputVolumeDb(db: outputVolumeDbValue);
    setEchoCancellation(on_: echoCancellation);
    setLevelNormalisation(on_: normaliseLevels);
    setReverb(on_: reverb);
    setFeedbackGuard(mode: feedbackGuard);
    if (selectedInput != null || selectedOutput != null) {
      await setAudioDevices(input: selectedInput, output: selectedOutput);
    }
  }

  Future<void> _loadServers(SharedPreferences prefs) async {
    _deleted
      ..clear()
      ..addAll(_decodeTombstones(prefs.getString(_prefsDeleted)));
    _settingStamps
      ..clear()
      ..addAll(_decodeTombstones(prefs.getString(_prefsSettingStamps)));
    _publishedSettings = _syncedSettings();

    final raw = prefs.getStringList(_prefsKey) ?? const [];
    servers
      ..clear()
      ..addAll(
        raw.map(
          (s) => SavedServer.fromJson(jsonDecode(s) as Map<String, dynamic>),
        ),
      );

    for (final s in servers.take(maxServers)) {
      await _register(s);
    }
  }

  /// Saves to disk and, unless told otherwise, queues an upload.
  ///
  /// [publish] is false only when the write is itself the result of a sync:
  /// storing what the merge produced and immediately offering it back as news
  /// would have two devices talking past each other indefinitely.
  Future<void> _persist({bool publish = true}) async {
    _stampChangedSettings();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString(_prefsDeleted, jsonEncode(_deleted));
    await prefs.setString(_prefsSettingStamps, jsonEncode(_settingStamps));
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
    if (publish) _scheduleSync();
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
    // The same host is a legitimate second entry: a different username, a
    // different default channel, or simply a spare. Only the key has to be
    // unique, so a colliding one is renamed rather than refused.
    final entry =
        (servers.any((e) => e.id == s.id)
                ? s.copyWith(localId: _uniqueId(s.host, s.port))
                : s)
            .stamped();
    servers.add(entry);
    // Only the first `maxServers` get a live session; the rest stay as saved
    // entries the user can swap in.
    if (runtimes.length < maxServers) {
      await _register(entry);
    }
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  /// Whether a saved server may be edited or removed.
  ///
  /// Only while it is genuinely disconnected вЂ” idle, stopped, or given up.
  /// Connection details are baked into a session when it is registered, so
  /// changing them means tearing the session down and building another, which
  /// from the rider's side is an unannounced drop in the middle of a
  /// conversation. Removing one is worse: the session keeps running with
  /// nothing in the list pointing at it. Both are avoided by declining until
  /// the user has hung up, which is one deliberate tap away.
  bool canModifyServer(String id) => runtimeFor(id).isModifiable;

  /// Replaces a saved server in place, keeping its key so the live session and
  /// anything pointing at it stay attached to the same entry.
  Future<String?> updateServer(SavedServer updated) async {
    final index = servers.indexWhere((e) => e.id == updated.id);
    if (index < 0) return 'That server is no longer in your list.';
    // Checked here as well as in the UI: the editor can be left open across a
    // reconnect, so the state that allowed it to open may be gone by the time
    // it is saved.
    if (!canModifyServer(updated.id)) return _strings.serverBusyChange;

    servers[index] = updated.stamped();
    await _persist();

    // Connection details are baked into the session when it is registered, so
    // a changed host or username needs the session rebuilt rather than nudged.
    try {
      await removeServer(serverId: updated.id);
    } catch (_) {
      // Never registered, which is fine вЂ” _register puts it back either way.
    }
    runtimes.remove(updated.id);
    await _register(updated);

    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  /// Imports servers from a `mumble://` link or JSON profile text.
  Future<String?> importFromText(
    String text, {
    String? fallbackUsername,
  }) async {
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
        servers.add(s.stamped());
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
    final client = SystemProxy.instance.createClient();
    try {
      final uri = Uri.parse(url.trim());
      final res = await client.get(uri).timeout(const Duration(seconds: 20));
      if (res.statusCode != 200) {
        return 'Download failed (HTTP ${res.statusCode}).';
      }
      return importFromText(res.body);
    } catch (e) {
      return 'Could not download that file: $e';
    } finally {
      client.close();
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
    final copy = s
        .copyWith(name: '${s.name} (copy)', localId: _uniqueId(s.host, s.port))
        .stamped();
    servers.add(copy);
    if (runtimes.length < maxServers) await _register(copy);
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  // --- syncing between the user's devices -----------------------------------

  /// Whether to use the platform's sync facility, where there is one.
  bool cloudSync = true;

  /// Whether that facility is actually usable вЂ” signed in, and switched on for
  /// this app. Distinct from [cloudSync]: the user can want this and still not
  /// have it, and being told which is which is the difference between a
  /// setting that looks broken and one that explains itself.
  bool cloudReady = false;

  /// What went wrong last time, if anything.
  String? cloudError;

  /// Deletions, kept so they can outlive the entry and reach other devices.
  final Map<String, int> _deleted = {};

  Timer? _syncTimer;
  bool _syncing = false;

  /// Servers whose details changed under a session that was in use, so the
  /// rebuild was deferred until it is not.
  final Set<String> _pendingRegistration = {};

  CloudKind get cloudKind => CloudSync.instance.kind;

  /// Whether a Bluetooth remote's media buttons arrive as taps rather than as
  /// a press and a release.
  ///
  /// True only on iOS, where those buttons are not key events at all and reach
  /// the app through the remote command centre, which reports that a button
  /// was used and never that it is still down.
  bool get remoteButtonsAreTapsOnly {
    if (kIsWeb) return false;
    try {
      return Platform.isIOS;
    } catch (_) {
      return false;
    }
  }

  /// Whether this platform lets the user pick an audio device at all.
  ///
  /// Phones and tablets expose one logical route and switch it themselves as
  /// headsets come and go, so there is nothing to enumerate and nothing a
  /// re-check could turn up. A desktop with a single device is a different
  /// case that looks identical from a device count: there the list really can
  /// change, and asking again is the way to find out.
  bool get canPickAudioDevices {
    if (kIsWeb) return false;
    try {
      return !(Platform.isIOS || Platform.isAndroid);
    } catch (_) {
      return true;
    }
  }

  Future<void> setCloudSync(bool on) async {
    cloudSync = on;
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsCloudSync, on);
    if (on) await syncNow();
  }

  static Map<String, int> _decodeTombstones(String? raw) {
    if (raw == null || raw.isEmpty) return const {};
    try {
      final j = jsonDecode(raw);
      if (j is! Map) return const {};
      return {
        for (final e in j.entries)
          if (e.value is num) '${e.key}': (e.value as num).toInt(),
      };
    } catch (_) {
      return const {};
    }
  }

  /// Publishes shortly, rather than at once.
  ///
  /// Editing a server saves on every keystroke, and each save would otherwise
  /// be a round trip to iCloud carrying a half-typed hostname. Waiting for the
  /// typing to stop sends one copy of the finished thing.
  void _scheduleSync() {
    if (!cloudSync || !CloudSync.instance.isLive) return;
    _syncTimer?.cancel();
    _syncTimer = Timer(const Duration(seconds: 2), () => unawaited(syncNow()));
  }

  /// Reconciles this device's list with the cloud's, both ways.
  ///
  /// Deliberately one path for both directions. Reading and writing are the
  /// same operation seen from either end вЂ” merge, keep what came of it, and
  /// publish it if it differs from what was there вЂ” and splitting them into a
  /// download and an upload invites the two to disagree about what a merge
  /// means.
  Future<bool> syncNow() async {
    if (!cloudSync || !CloudSync.instance.isLive) return false;
    // Reads are cheap but applying one is not, and a burst of remote-change
    // notifications during the initial pull would otherwise have several
    // merges rebuilding the same sessions underneath each other.
    if (_syncing) return false;
    _syncing = true;
    try {
      final blob = await CloudSync.instance.read();
      cloudReady = await CloudSync.instance.isReady();

      final mine = _localSnapshot();
      final theirs = _withPasswords(
        SyncSnapshot.decode(blob?.payload) ?? const SyncSnapshot(),
        blob?.secrets ?? const {},
      );
      final merged = mergeSnapshots(
        mine,
        theirs,
        nowMs: DateTime.now().millisecondsSinceEpoch,
      );

      if (!sameSnapshot(merged, mine)) await _applyMerged(merged);
      await _applySyncedSettings(merged.settings);
      await _applySyncedSettings(merged.settings);
      if (!sameSnapshot(merged, theirs)) {
        final (payload, secrets) = _withoutPasswords(merged);
        cloudError = await CloudSync.instance.write(
          CloudBlob(payload: payload.encode(), secrets: secrets),
          liveIds: [for (final s in merged.servers) syncIdOf(s)],
        );
      } else {
        cloudError = null;
      }
      notifyListeners();
      return true;
    } catch (e) {
      cloudError = e.toString();
      notifyListeners();
      return false;
    } finally {
      _syncing = false;
    }
  }

  SyncSnapshot _localSnapshot() => SyncSnapshot(
    servers: [for (final s in servers) s.toJson()],
    deleted: Map.of(_deleted),
    settings: {
      for (final e in _syncedSettings().entries)
        e.key: {'v': e.value, 'at': _settingStamps[e.key] ?? 0},
    },
  );

  /// When each setting was last changed here.
  final Map<String, int> _settingStamps = {};

  /// The values as they were at the last save, so a change can be spotted
  /// without every setter having to remember to say so.
  Map<String, Object?> _publishedSettings = const {};

  /// The settings worth carrying between a rider's devices.
  ///
  /// Deliberately not everything. The chosen input and output devices are
  /// named after hardware that exists on one machine and not another, so
  /// syncing them would have a laptop pointing at a microphone it does not
  /// have. Nor the sync switch itself: turning it off on one device would
  /// otherwise turn it off everywhere, which is the one thing that cannot be
  /// undone remotely.
  Map<String, Object?> _syncedSettings() => {
    'noise': noise.index,
    'micMode': micMode.index,
    'inputGain': inputGainDbValue,
    'outputVolume': outputVolumeDbValue,
    'echoCancellation': echoCancellation,
    'normaliseLevels': normaliseLevels,
    'reverb': reverb,
    'feedbackGuard': feedbackGuard.index,
    'floatingWindow': _wantOverlay,
    'locale': _locale?.languageCode,
    'proxyEnabled': SystemProxy.instance.enabled,
    'proxyManual': SystemProxy.instance.manualProxy,
    'buttons': jsonEncode([for (final b in buttons.bindings) b.toJson()]),
  };

  /// Stamps whatever has changed since the last save.
  ///
  /// Done by comparing values rather than by every setter announcing itself:
  /// there are a dozen of them, and the one that forgets is the one that
  /// silently stops syncing.
  void _stampChangedSettings() {
    final now = DateTime.now().millisecondsSinceEpoch;
    final current = _syncedSettings();
    for (final e in current.entries) {
      if (_publishedSettings[e.key] != e.value) {
        _settingStamps[e.key] = now;
      }
    }
    _publishedSettings = current;
  }

  /// Adopts settings that arrived from another device.
  ///
  /// Only what actually differs, and without stamping: applying a change is
  /// not making one, and treating it as one would have two devices handing the
  /// same value back and forth with a fresh timestamp every round.
  Future<void> _applySyncedSettings(Map<String, dynamic> merged) async {
    final prefs = await SharedPreferences.getInstance();
    var audioChanged = false;
    var changed = false;

    T? read<T>(String key) {
      final entry = merged[key];
      if (entry is! Map) return null;
      final value = entry['v'];
      return value is T ? value : null;
    }

    if (read<int>('noise') case final v?
        when v >= 0 &&
            v < NoiseSetting.values.length &&
            NoiseSetting.values[v] != noise) {
      noise = NoiseSetting.values[v];
      await prefs.setInt(_prefsNoise, v);
      setNoise(noise: noise);
      changed = true;
    }
    if (read<int>('micMode') case final v?
        when v >= 0 &&
            v < MicMode.values.length &&
            MicMode.values[v] != micMode) {
      micMode = MicMode.values[v];
      await prefs.setInt(_prefsMic, v);
      setMicMode(mode: micMode);
      changed = true;
    }
    if (read<double>('inputGain') case final v? when v != inputGainDbValue) {
      inputGainDbValue = v;
      await prefs.setDouble(_prefsInputGain, v);
      audioChanged = true;
    }
    if (read<double>('outputVolume') case final v?
        when v != outputVolumeDbValue) {
      outputVolumeDbValue = v;
      await prefs.setDouble(_prefsOutputVolume, v);
      audioChanged = true;
    }
    if (read<bool>('echoCancellation') case final v?
        when v != echoCancellation) {
      echoCancellation = v;
      await prefs.setBool(_prefsEchoCancellation, v);
      audioChanged = true;
    }
    if (read<bool>('normaliseLevels') case final v? when v != normaliseLevels) {
      normaliseLevels = v;
      await prefs.setBool(_prefsNormaliseLevels, v);
      audioChanged = true;
    }
    if (read<bool>('reverb') case final v? when v != reverb) {
      reverb = v;
      await prefs.setBool(_prefsReverb, v);
      audioChanged = true;
    }
    if (read<int>('feedbackGuard') case final v?
        when v >= 0 &&
            v < FeedbackGuardMode.values.length &&
            FeedbackGuardMode.values[v] != feedbackGuard) {
      feedbackGuard = FeedbackGuardMode.values[v];
      await prefs.setInt(_prefsFeedbackGuard, v);
      audioChanged = true;
    }
    if (read<String>('locale') case final v?
        when v != _locale?.languageCode &&
            supportedLocales.any((l) => l.languageCode == v)) {
      _locale = Locale(v);
      await prefs.setString(_prefsLocale, v);
      changed = true;
    }
    if (read<bool>('proxyEnabled') case final v?
        when v != SystemProxy.instance.enabled) {
      SystemProxy.instance.enabled = v;
      await prefs.setBool(_prefsProxyEnabled, v);
      changed = true;
    }
    if (read<String>('buttons') case final v?
        when v != jsonEncode([for (final b in buttons.bindings) b.toJson()])) {
      try {
        buttons.setBindings([
          for (final j in jsonDecode(v) as List)
            ?ButtonBinding.fromJson(j as Map<String, dynamic>),
        ]);
        await prefs.setString(_prefsButtons, v);
        changed = true;
      } catch (_) {
        // Bindings from a build that wrote them differently. Keeping ours is
        // better than losing them to a parse error.
      }
    }

    if (audioChanged) {
      await _applyAudioSettings();
      changed = true;
    }
    // Recorded as published so the next save does not read these back as local
    // edits and stamp them all over again.
    _publishedSettings = _syncedSettings();
    if (changed) notifyListeners();
  }

  /// Lifts passwords out of the list, to be stored somewhere better protected.
  ///
  /// Which store that is, and why it is a different one, is the platform's
  /// business вЂ” see `shared/CloudStore.swift`. All that matters here is that a
  /// password never goes into the payload.
  static (SyncSnapshot, Map<String, String>) _withoutPasswords(SyncSnapshot s) {
    final secrets = <String, String>{};
    final servers = <Map<String, dynamic>>[];
    for (final e in s.servers) {
      final copy = Map<String, dynamic>.of(e);
      final password = copy.remove('password');
      if (password is String && password.isNotEmpty) {
        secrets[syncIdOf(e)] = password;
      }
      servers.add(copy);
    }
    return (SyncSnapshot(servers: servers, deleted: s.deleted), secrets);
  }

  /// Puts them back, before the merge rather than after.
  ///
  /// A remote entry has to arrive whole for the comparison to be fair: reunite
  /// it afterwards and an incoming entry that wins on recency wins as a server
  /// with no password, silently discarding one that was never lost.
  static SyncSnapshot _withPasswords(
    SyncSnapshot s,
    Map<String, String> secrets,
  ) => SyncSnapshot(
    deleted: s.deleted,
    servers: [
      for (final e in s.servers)
        if (secrets[syncIdOf(e)] case final password?)
          {...e, 'password': password}
        else
          e,
    ],
  );

  /// Adopts a merged list, rebuilding only the sessions that need it.
  Future<void> _applyMerged(SyncSnapshot merged) async {
    final before = {for (final s in servers) s.id: s};
    final next = [for (final j in merged.servers) SavedServer.fromJson(j)];

    servers
      ..clear()
      ..addAll(next);
    _deleted
      ..clear()
      ..addAll(merged.deleted);
    await _persist(publish: false);

    for (final id in before.keys) {
      if (next.any((s) => s.id == id)) continue;
      runtimes.remove(id);
      try {
        await removeServer(serverId: id);
      } catch (_) {
        // Never registered вЂ” only the first few entries ever are.
      }
    }

    for (final s in next.take(maxServers)) {
      final old = before[s.id];
      if (old == null) {
        if (runtimes.length < maxServers) await _register(s);
        continue;
      }
      // A renamed server keeps talking. Connection details are baked into the
      // session when it is registered, so only those warrant a rebuild.
      if (old.sameConnection(s)) continue;

      // But never mid-call. A conversation is not worth interrupting for a
      // detail somebody altered on a laptop, and the change is not urgent вЂ”
      // it applies at the next connect, which is when it first matters.
      //
      // This is also the backstop against a merge that keeps changing its
      // mind. One did: a stale field kept winning, the entry looked different
      // every round, and each round tore down the connection and rebuilt it,
      // about once a second, until the server started refusing us. Whatever
      // decides the merge, it cannot reach through this.
      final rt = runtimeFor(s.id);
      if (rt.isLive || rt.isBusy) {
        _pendingRegistration.add(s.id);
        continue;
      }
      try {
        await removeServer(serverId: s.id);
      } catch (_) {}
      runtimes.remove(s.id);
      await _register(s);
    }

    notifyListeners();
    unawaited(refreshPings());
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
          XTypeGroup(label: 'JSON', extensions: ['json']),
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
          XTypeGroup(label: 'Server profiles', extensions: ['json', 'mumble']),
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

  /// Removes a server from the list. Returns why not, when it declines.
  Future<String?> forgetServer(String id) async {
    if (!canModifyServer(id)) return _strings.serverBusyChange;

    // Recorded before the entry goes, so the other devices are told it was
    // deleted rather than left to notice it missing and put it back.
    _deleted[id] = DateTime.now().millisecondsSinceEpoch;
    servers.removeWhere((s) => s.id == id);
    runtimes.remove(id);
    try {
      await removeServer(serverId: id);
    } catch (_) {}
    await _persist();
    notifyListeners();
    return null;
  }

  Future<void> connect(String id) async {
    // Details that arrived from another device while this session was in use
    // are applied now, on the way in, rather than having interrupted it then.
    if (_pendingRegistration.remove(id)) {
      final i = servers.indexWhere((s) => s.id == id);
      if (i >= 0) {
        try {
          await removeServer(serverId: id);
        } catch (_) {}
        runtimes.remove(id);
        await _register(servers[i]);
      }
    }
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
      // Stamped, because this is a change to the entry like any other. Without
      // it the edit carries the old timestamp, ties with the copy in the cloud
      // that predates it, and loses to whatever the tie-break happens to
      // prefer вЂ” so the trust the user just granted gets rolled back.
      servers[i] = servers[i].copyWith(certFingerprint: fp).stamped();
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
    await Future.wait(
      snapshot.map((s) async {
        try {
          final status = await pingServer(
            serverId: s.id,
            host: s.host,
            port: s.port,
          );
          runtimeFor(s.id).probe = status;
        } catch (_) {
          // Probing is best-effort; a failure just leaves the previous reading.
        }
      }),
    );
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

  /// Acoustic echo cancellation. On by default, because a speaker in the same
  /// room as the microphone is the common case.
  bool echoCancellation = true;

  /// A short room tail under incoming voices. On by default: a gated voice
  /// cutting off mid-breath is the unnatural option, not this.
  bool reverb = true;

  Future<void> setReverbEnabled({required bool value}) async {
    reverb = value;
    setReverb(on_: value);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsReverb, value);
  }

  /// Levels incoming speakers towards a common loudness.
  bool normaliseLevels = true;

  Future<void> setNormaliseLevels({required bool value}) async {
    normaliseLevels = value;
    setLevelNormalisation(on_: value);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsNormaliseLevels, value);
  }

  Future<void> setEchoCancellationEnabled({required bool value}) async {
    echoCancellation = value;
    setEchoCancellation(on_: value);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsEchoCancellation, value);
  }

  /// Which guard is applied to what the echo canceller could not remove.
  ///
  /// Off by default: cancellation alone is enough on most headsets, and every
  /// one of these costs something вЂ” half duplex, a hard cut, or a quieter
  /// talker вЂ” which is not worth paying until there is a fault to fix.
  FeedbackGuardMode feedbackGuard = FeedbackGuardMode.off;

  Future<void> updateFeedbackGuard(FeedbackGuardMode mode) async {
    feedbackGuard = mode;
    setFeedbackGuard(mode: mode);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_prefsFeedbackGuard, mode.index);
  }

  void testOutput() => playTestTone(millis: 700);

  void setTransmit(bool on) {
    if (_transmitting == on) return;
    _transmitting = on;
    setTransmitting(on_: on);
    notifyListeners();
    _pushOverlay();
  }

  // --- hardware and Bluetooth buttons -------------------------------------

  final ButtonController buttons = ButtonController.instance;

  List<ButtonBinding> get buttonBindings => buttons.bindings;

  void _setUpButtons(SharedPreferences prefs) {
    buttons.onCaptureChanged = notifyListeners;
    buttons.onTransmit = setTransmit;
    buttons.onToggleMute = toggleMute;
    buttons.onToggleDeafen = toggleDeafen;

    final raw = prefs.getStringList(_prefsButtons) ?? const [];
    final loaded = <ButtonBinding>[];
    for (final s in raw) {
      final b = ButtonBinding.fromJson(jsonDecode(s) as Map<String, dynamic>);
      if (b != null) loaded.add(b);
    }
    buttons.setBindings(loaded);
    buttons.install();
  }

  Future<void> _persistBindings() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setStringList(
      _prefsButtons,
      buttons.bindings.map((b) => jsonEncode(b.toJson())).toList(),
    );
  }

  /// Waits for the next button press and binds it to [action].
  void learnButton(ButtonAction action, void Function(ButtonBinding) onBound) {
    buttons.learnNext((keyId, label) async {
      final binding = ButtonBinding(keyId: keyId, action: action, label: label);
      buttons.addBinding(binding);
      await _persistBindings();
      notifyListeners();
      onBound(binding);
    });
    notifyListeners();
  }

  void cancelLearningButton() {
    buttons.cancelLearning();
    notifyListeners();
  }

  Future<void> removeButtonBinding(int keyId) async {
    buttons.removeBinding(keyId);
    await _persistBindings();
    notifyListeners();
  }

  // --- floating island ---------------------------------------------------

  final OverlayBridge overlay = OverlayBridge.instance;

  /// Whether the floating call window should be showing.
  ///
  /// On by default and remembered between launches. It is the control a rider
  /// on a bike depends on and the one they are least able to go and switch on
  /// again, so it defaults to present rather than to absent вЂ” and having to
  /// turn it on after every launch made it look as though it kept failing.
  bool overlayEnabled = false;
  String _lastOverlaySignature = '';

  bool get overlaySupported => overlay.isSupported;
  FloatingKind get overlayKind => overlay.kind;

  /// Shows the floating window. Returns an error message, or null on success.
  Future<String?> enableOverlay() async {
    if (!overlay.isSupported) {
      return 'Floating windows are not available on this platform.';
    }
    if (!await overlay.hasPermission()) {
      // Android only grants this from its own settings screen.
      await overlay.requestPermission();
      if (!await overlay.hasPermission()) {
        return 'Allow "display over other apps" for MumbleWay, then try again.';
      }
    }

    // The window is another view onto the same state, not a second source of
    // truth: every control routes back through here and waits to be told the
    // result, so the two can never drift apart.
    overlay.onTransmit = setTransmit;
    overlay.onToggleMute = toggleMute;
    overlay.onToggleDeafen = toggleDeafen;
    overlay.onHangup = hangupAll;
    overlay.onDismissed = () {
      // Deliberately does not turn the setting off. The window closes whenever
      // the app comes back to the front, which is not the user saying they no
      // longer want it вЂ” and having the switch flip itself off every time they
      // looked at the app made it read as broken.
      notifyListeners();
    };
    overlay.onStatus = (message) {
      overlayStatus = message;
      notifyListeners();
    };

    overlayStatus = null;
    await overlay.setPhrases(_overlayPhrases());
    final error = await overlay.show();
    overlayEnabled = error == null;
    if (error == null) unawaited(_rememberOverlayChoice(true));
    notifyListeners();
    _lastOverlaySignature = '';
    _pushOverlay();
    return error;
  }

  /// Why the floating window did not appear, or null.
  ///
  /// Separate from the error [enableOverlay] returns, because the interesting
  /// failures happen after it has already reported success вЂ” the window is
  /// requested, the system declines, and nothing was ever going to be thrown.
  String? overlayStatus;

  /// What the user asked for, as opposed to what is currently showing.
  ///
  /// The two differ while the window is closed but the setting is still on,
  /// which is the normal state whenever the app is in the foreground.
  bool _wantOverlay = true;

  Future<void> _rememberOverlayChoice(bool want) async {
    _wantOverlay = want;
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsFloatingWindow, want);
  }

  Future<void> disableOverlay() async {
    unawaited(_rememberOverlayChoice(false));
    overlayStatus = null;
    await overlay.hide();
    overlayEnabled = false;
    notifyListeners();
  }

  /// Leaves every connected server. The floating window's hang-up is not tied
  /// to one slot, because from the window there is no way to say which.
  Future<void> hangupAll() async {
    for (final server in servers.toList()) {
      if (runtimeFor(server.id).isLive) {
        await disconnect(server.id);
      }
    }
  }

  /// Pushes the call state onto the floating window, skipping the call when
  /// nothing visible has changed вЂ” this runs on every roster update.
  void _pushOverlay() {
    if (!overlayEnabled) return;
    final names = allSpeakingNames;
    final speakers = [
      for (final rt in runtimes.values)
        if (rt.isLive)
          for (final u in rt.channelPeers)
            if (rt.speakerLevels[u.session] case final db?)
              (name: u.name, levelDb: db),
    ];
    final connectedCount = runtimes.values.where((r) => r.isLive).length;
    final reconnectingCount = runtimes.values
        .where((r) => r.status == ConnStatus.reconnecting || r.isBusy)
        .length;
    final failedCount = runtimes.values
        .where((r) => r.status == ConnStatus.failed)
        .length;
    final connected = connectedCount > 0;

    final l = _strings;
    // Assembled here rather than in the drawing code, because a sentence with
    // a number in it has to agree with itself, and the rules for that belong
    // with the language.
    final String connectionText;
    final int connectionLevel;
    if (reconnectingCount > 0) {
      connectionLevel = 2;
      connectionText = connectedCount > 0
          ? l.pipUpAndReconnecting(connectedCount, reconnectingCount)
          : l.pipReconnecting;
    } else if (connectedCount > 0) {
      connectionLevel = 1;
      connectionText = connectedCount == 1
          ? l.pipConnected
          : l.pipConnectedCount(connectedCount);
    } else if (failedCount > 0) {
      connectionLevel = 3;
      connectionText = l.pipNoConnection;
    } else {
      connectionLevel = 0;
      connectionText = l.pipNotConnected;
    }

    // Spelled out rather than taken from the enum's index: the Rust order is
    // voice-activity, push-to-talk, continuous, so an index would tell the
    // window that push-to-talk was hands-free and hand it the opposite
    // behaviour of the one chosen.
    final micModeCode = switch (micMode) {
      MicMode.pushToTalk => 0,
      MicMode.voiceActivity => 1,
      MicMode.continuous => 2,
    };

    // Whether the microphone is actually open, by whatever route. In the
    // hands-free modes nobody is holding anything and it opens anyway, so the
    // on-air light cannot be driven from the talk button.
    final live = switch (micMode) {
      MicMode.pushToTalk => _transmitting,
      MicMode.voiceActivity => connected && !_muted && _speaking,
      MicMode.continuous => connected && !_muted,
    };
    // How many other people are within earshot, across every server at once.
    //
    // Silence on the right half is ambiguous: nobody is talking, but the rider
    // cannot tell whether that means the channel is quiet or that they are on
    // their own — and those call for quite different reactions at 100 km/h.
    // Summed across servers because from the rider's side it is one
    // conversation, however many connections carry it.
    final otherCount = connected
        ? runtimes.values
              .where((r) => r.isLive)
              .fold(0, (sum, r) => sum + r.channelPeers.length)
        : 0;
    final othersOnline = !connected
        ? ''
        : otherCount > 0
        ? l.pipOthersOnline(otherCount)
        : l.pipNobodyElse;

    // The level arrives ten times a second and never repeats exactly, so it is
    // rounded to whole decibels before being compared. Without that the
    // signature always differs and the check stops filtering anything.
    final signature =
        '${speakers.map((s) => '${s.name}:${s.levelDb.round()}').join(',')}'
        '|$connectionText|$othersOnline|$_transmitting|$live|$micModeCode|$connectedCount|$reconnectingCount|$failedCount|$_muted'
        '|$_deafened|$_speaking|${_inputLevelDb.round()}'
        '|${_thresholdDb.round()}|${_noiseFloorDb.round()}';
    if (signature == _lastOverlaySignature) return;
    _lastOverlaySignature = signature;
    unawaited(
      overlay.update(
        names: names,
        speakers: speakers,
        transmitting: _transmitting,
        micMode: micModeCode,
        live: live,
        connected: connected,
        connectionText: connectionText,
        connectionLevel: connectionLevel,
        moreSpeakers: speakers.length > 4
            ? l.pipMoreSpeakers(speakers.length - 4)
            : '',
        othersOnline: othersOnline,
        connectedCount: connectedCount,
        reconnectingCount: reconnectingCount,
        failedCount: failedCount,
        muted: _muted,
        deafened: _deafened,
        levelDb: _inputLevelDb,
        thresholdDb: _thresholdDb,
        noiseFloorDb: _noiseFloorDb,
        speaking: _speaking,
      ),
    );
  }

  /// Whether the diagnostics panel is showing.
  ///
  /// Deliberately not persisted: it is a thing you open while chasing a
  /// problem, not a preference, and finding it still up next launch would be
  /// a small puzzle every time.
  bool diagnosticsOpen = false;

  void toggleDiagnostics() {
    diagnosticsOpen = !diagnosticsOpen;
    notifyListeners();
  }

  /// Sounds a cue for anyone who has joined or left our channel.
  ///
  /// Compared here rather than in the core because this is the only place that
  /// knows which channel we are in and who was in it a moment ago; the server
  /// sends a whole roster and leaves the difference to the reader.
  void _announceChannelChanges(ServerRuntime rt) {
    final channel = rt.currentChannelId;
    if (channel == null) {
      // Not in a channel yet; nothing to compare against.
      rt.knownPeers = null;
      rt.knownPeersChannel = null;
      return;
    }

    // Only compare against a roster that includes us. The server sends state
    // for individual users as well as whole rosters, and both arrive as the
    // same event вЂ” so a partial one replaced the list with a single person,
    // making everybody else appear to leave and then arrive again a moment
    // later. With three devices on one server that produced a cue every few
    // seconds, which is the opposite of the point.
    if (!rt.users.any((u) => u.session == rt.selfSession)) return;

    final now = rt.channelPeers.map((u) => u.session).toSet();
    final before = rt.knownPeersChannel == channel ? rt.knownPeers : null;
    rt.knownPeers = now;
    rt.knownPeersChannel = channel;
    if (before == null) return;

    // And never more than one cue a second, whatever the rosters say. A cue
    // that can repeat is a cue that can become a metronome, and a rider cannot
    // reach the phone to stop it.
    final now_ = DateTime.now();
    if (rt.lastCueAt != null &&
        now_.difference(rt.lastCueAt!) < const Duration(seconds: 1)) {
      return;
    }
    rt.lastCueAt = now_;

    // One cue per event, not per person: three people leaving at once is one
    // thing happening, and three overlapping tones is just a noise.
    if (now.difference(before).isNotEmpty) {
      playParticipantCue(joined: true);
    }
    if (before.difference(now).isNotEmpty) {
      playParticipantCue(joined: false);
    }
  }

  void toggleMute() {
    _muted = !_muted;
    setMicrophoneMuted(muted: _muted);
    _pushOverlay();
    notifyListeners();
  }

  void toggleDeafen() {
    _deafened = !_deafened;
    setDeafened(deafened: _deafened);
    _pushOverlay();
    notifyListeners();
  }

  Future<void> updateNoise(NoiseSetting v) async {
    noise = v;
    setNoise(noise: v);
    await _persist();
    notifyListeners();
  }

  Future<void> updateMicMode(MicMode v) async {
    micMode = v;
    setMicMode(mode: v);
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
  ///   advertise gzip вЂ” which looks exactly like the service being down. Dart's
  ///   `HttpClient` sends it by default, and it is requested explicitly here so
  ///   that stays true if the client is ever swapped out.
  /// * **`version` is required** вЂ” without it the endpoint also returns 501.
  ///
  /// [usedFallback] reports whether the network request failed and a small
  /// built-in list was substituted.
  Future<(List<PublicServer>, bool usedFallback)> fetchPublicServers() async {
    // Through the system proxy where one is configured: on a machine behind a
    // proxy the direct route usually fails outright.
    final client = SystemProxy.instance.createClient();
    try {
      final uri = Uri.parse(
        'https://publist.mumble.info/v1/list?version=1.5.735',
      );
      final res = await client
          .get(uri, headers: {'Accept-Encoding': 'gzip', 'Accept': '*/*'})
          .timeout(const Duration(seconds: 20));

      if (res.statusCode == 200 && res.body.contains('<server')) {
        final parsed = _parsePublicList(res.body);
        if (parsed.isNotEmpty) return (parsed, false);
      }
    } catch (_) {
      // Network trouble; fall through to the built-in list.
    } finally {
      client.close();
    }
    return (_knownPublicServers, true);
  }

  // --- proxy --------------------------------------------------------------

  /// Whether outbound HTTP goes through the OS proxy. On by default.
  bool get proxyEnabled => SystemProxy.instance.enabled;

  /// Human-readable description of what is currently in effect.
  String get proxyDescription => SystemProxy.instance.config.description;

  Future<void> setProxyEnabled(bool on) async {
    SystemProxy.instance.enabled = on;
    await SystemProxy.instance.refresh();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsProxyEnabled, on);
    notifyListeners();
  }

  Future<void> setManualProxy(String? hostPort) async {
    SystemProxy.instance.manualProxy = hostPort;
    await SystemProxy.instance.refresh();
    final prefs = await SharedPreferences.getInstance();
    if (hostPort == null || hostPort.trim().isEmpty) {
      await prefs.remove(_prefsProxyManual);
    } else {
      await prefs.setString(_prefsProxyManual, hostPort.trim());
    }
    notifyListeners();
  }

  String? get manualProxy => SystemProxy.instance.manualProxy;

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
      out.add(
        PublicServer(
          name: attrs['name'] ?? host,
          host: host,
          port: port,
          country: attrs['country'] ?? '',
        ),
      );
    }
    return out;
  }

  static const List<PublicServer> _knownPublicServers = [
    PublicServer(
      name: 'Mumble.info (official test)',
      host: 'mumble.info',
      port: 64738,
    ),
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
        // The core reports the wait once, when it starts. Turning it into a
        // deadline is what lets the UI count down instead of showing the same
        // number until the next event arrives.
        rt.retryDeadline = field0.retryInMs > BigInt.zero
            ? DateTime.now().add(Duration(milliseconds: rt.retryInMs))
            : null;
      case AppEvent_Users(:final serverId, :final users):
        final rt = runtimeFor(serverId);
        rt.users = users;
        _announceChannelChanges(rt);
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
        :final thresholdDb,
        :final noiseFloorDb,
      ):
        // Paced exactly like every other meter, so the microphone and the
        // participants fade at the same rate rather than one snapping while
        // the others slide.
        _inputLevelDb = VoiceMeter.follow(_inputLevelDb, levelDb);
        _speaking = speaking;
        _thresholdDb = thresholdDb;
        _noiseFloorDb = noiseFloorDb;
        _pushOverlay();
      case AppEvent_SpeakerLevels(:final levels):
        final reported = <String, Set<int>>{};
        for (final entry in levels) {
          runtimeFor(
            entry.serverId,
          ).noteSpeakerLevel(entry.session, entry.levelDb);
          (reported[entry.serverId] ??= <int>{}).add(entry.session);
        }
        // A speaker who stops is reaped from the mixer and simply stops being
        // reported, so without this their meter freezes at whatever it last
        // showed вЂ” a full bar for someone who went quiet a minute ago.
        for (final entry in runtimes.entries) {
          entry.value.decayUnreported(reported[entry.key] ?? const <int>{});
        }
        _pushOverlay();
      case AppEvent_Moderated(:final muted, :final deafened, :final by):
        // The cue already played in the core; this is the visible half.
        final what = deafened != null
            ? (deafened ? 'deafened you' : 'undeafened you')
            : (muted == true ? 'muted you' : 'unmuted you');
        lastModerationMessage = '$by $what';
      case AppEvent_Certificate(
        :final serverId,
        :final fingerprint,
        :final changed,
      ):
        final rt = runtimeFor(serverId);
        rt
          ..pendingFingerprint = fingerprint
          ..certificateChanged = changed;
        if (!changed) {
          final i = servers.indexWhere((s) => s.id == serverId);
          if (i >= 0 && servers[i].certFingerprint == null) {
            servers[i] = servers[i]
                .copyWith(certFingerprint: fingerprint)
                .stamped();
            unawaited(_persist());
          }
        }
      case AppEvent_Welcome(:final serverId, :final text):
        runtimeFor(serverId).welcome = text;
      case AppEvent_Log(:final entries):
        // Its own notifier, so a burst of log lines does not rebuild the whole
        // app: only the panel drawing them is listening.
        EngineLog.instance.add(entries);
        return;
    }
    notifyListeners();
    // Keep the island's speaker list in step with the roster.
    _pushOverlay();
  }

  @override
  void dispose() {
    _syncTimer?.cancel();
    _pingTimer?.cancel();
    _events?.cancel();
    super.dispose();
  }
}

/// Makes [AppState] available to the widget tree.
class AppStateScope extends InheritedNotifier<AppState> {
  const AppStateScope({
    super.key,
    required AppState state,
    required super.child,
  }) : super(notifier: state);

  static AppState of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<AppStateScope>();
    assert(scope != null, 'No AppStateScope found in context');
    return scope!.notifier!;
  }
}
