import 'dart:async';
import 'dart:convert';
import 'dart:io' show File, Platform;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/foundation.dart'
    show kIsWeb, listEquals, visibleForTesting;
import 'package:flutter/widgets.dart';
import 'package:path_provider/path_provider.dart';
import 'package:share_plus/share_plus.dart';
import 'package:shared_preferences/shared_preferences.dart';

import '../l10n/app_localizations.dart';
import '../services/server_refusal.dart';
import '../services/audio_session.dart';
import '../services/background_classifier.dart';
import '../services/button_controller.dart';
import '../services/cloud_sync.dart';
import '../services/device_identity.dart';
import '../services/engine_log.dart';
import '../services/overlay.dart';
import '../services/power.dart';
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
  /// stored explicitly so the same server can be kept more than once — under a
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
  /// else's business — and stamping those would have this device win conflicts
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
    defaultChannel: defaultChannel,
  );

  /// A draft entry from what the core parsed out of a link or a profile file.
  factory SavedServer.fromConfig(ServerConfig c) => SavedServer(
    name: c.name,
    host: c.host,
    port: c.port,
    username: c.username,
    password: c.password,
    certFingerprint: c.certFingerprint,
    defaultChannel: c.defaultChannel,
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

  /// The name this server knows us by.
  ///
  /// Read from the roster rather than from the saved entry, because the two can
  /// disagree: a server that already has a "Ilya" connected hands the second
  /// one "Ilya1", and the saved name would then be a quiet lie about who the
  /// other riders are hearing. Null until the roster arrives.
  String? get selfName {
    final me = selfSession;
    if (me == null) return null;
    for (final u in users) {
      if (u.session == me) return u.name;
    }
    return null;
  }

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

/// Notifier for the values that move at the audio frame rate.
///
/// The level meters change ten times a second for as long as the microphone is
/// open, and they are the only part of the interface that does. Driving them
/// from [AppState]'s own notifier meant the roster, the server cards, the
/// buttons and the title bar were all rebuilt twenty times a second to redraw
/// two bars — everything else in the frame being identical to the one before.
///
/// The precedent is [EngineLog], which was split out for the same reason: a
/// burst of log lines should not rebuild an app that is not showing the log.
class _MeterNotifier extends ChangeNotifier {
  /// [ChangeNotifier.notifyListeners] is protected, and this class exists to
  /// widen exactly that one call to the state object next door.
  void moved() => notifyListeners();
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
  static const _prefsJitterBuffer = 'mumbleway.jitterBufferMs';
  static const _prefsNamesRepaired = 'mumbleway.namesRepaired';
  static const _prefsReverb = 'mumbleway.reverb';
  static const _prefsSimpleModel = 'mumbleway.simpleModel';
  static const _prefsFeedbackGuard = 'mumbleway.feedbackGuard';
  static const _prefsDehiss = 'mumbleway.dehiss';
  static const _prefsSettingStamps = 'mumbleway.settingStamps';
  static const _prefsProxyEnabled = 'mumbleway.proxyEnabled';
  static const _prefsProxyManual = 'mumbleway.proxyManual';
  static const _prefsLocale = 'mumbleway.locale';
  static const _prefsButtons = 'mumbleway.buttonBindings';
  static const _prefsCloudSync = 'mumbleway.cloudSync';
  static const _prefsFloatingWindow = 'mumbleway.floatingWindow';
  static const _prefsDeleted = 'mumbleway.deletedServers';
  static const _prefsSuggestedName = 'mumbleway.suggestedName';

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

  /// Refusals from the server, as they arrive.
  ///
  /// A stream rather than a field, and deliberately: two refusals in a row are
  /// two things the user needs told, and a field would silently keep only the
  /// second. Broadcast because the listener is whatever screen is on top, and
  /// that changes.
  Stream<ServerRefusal> get refusals => _refusals.stream;
  final StreamController<ServerRefusal> _refusals =
      StreamController<ServerRefusal>.broadcast();

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

  /// `Automatic`, because it is now the option that decides best.
  ///
  /// It was `Helmet` on the reasoning that this is an app for riding and
  /// over-suppression is the safer error. That reasoning is now out of date:
  /// `Automatic` reaches `Helmet` within a second of hearing an engine, holds
  /// it for fifteen seconds after the engine stops, and takes a minute of
  /// quiet before it will go anywhere near the lightest profile — so a rider
  /// who never opens Settings gets `Helmet` when it matters and something
  /// kinder at the coffee stop.
  ///
  /// Only affects a fresh install. Anyone who has already chosen keeps their
  /// choice, which is the whole point of storing it.
  NoiseSetting noise = NoiseSetting.auto;

  /// Voice activation, because a rider has no free hand.
  ///
  /// Push-to-talk is the safer option in the narrow sense — a button cannot be
  /// tripped by a gust — but it asks for a hand on a handlebar at speed, and a
  /// default nobody can reach is not a safe default. It is also the only mode
  /// the onset look-ahead runs in, so this is what puts the first consonant of
  /// a sentence on the wire rather than the second.
  ///
  /// Only affects a fresh install. Anyone who has already chosen a mode has it
  /// in preferences and keeps it.
  MicMode micMode = MicMode.voiceActivity;
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
      'pipClose': l.pipClose,
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

  /// Marks the state as started up, without an engine behind it.
  ///
  /// Only the screenshot harness uses this. Every screen is behind
  /// [ready] — a state that has not started shows a spinner and nothing else —
  /// so without it a rendered screenshot is a picture of a loading indicator.
  @visibleForTesting
  void markReadyForTesting() {
    _ready = true;
    // The toolbar spins until the device has been measured, and nothing here
    // ever measures it — so without this the diagnostics icon in every store
    // screenshot is a progress indicator. Same reasoning as `_ready` above.
    _probeAnswered = true;
  }

  /// Pretends the devices opened, without an engine to open them.
  ///
  /// Runs the real [_syncReliefWatch], rather than reaching past it to the
  /// field it sets: the thing worth protecting is that opening the devices
  /// stops the toolbar spinner, and a test that set the flag itself would
  /// still pass with that wiring deleted.
  ///
  /// Cancel it with [dispose] — this starts the relief poll timer.
  @visibleForTesting
  void markAudioActiveForTesting() {
    _audioActive = true;
    _syncReliefWatch();
  }
  String? get startupError => _startupError;
  bool get muted => _muted;
  bool get deafened => _deafened;
  bool get transmitting => _transmitting;
  double get inputLevelDb => _inputLevelDb;

  /// Level voice activation opens at. Tracks the background noise, so it rises
  /// with engine and wind — which is what makes it worth showing.
  double get activationThresholdDb => _thresholdDb;

  /// Tracked background noise. The gap up to [activationThresholdDb] is the
  /// margin voice activation needs to clear.
  double get noiseFloorDb => _noiseFloorDb;
  bool get speaking => _speaking;

  /// Whether the talk button is relevant. In the automatic modes it is not,
  /// and the vertical space is better spent on the server list.
  bool get showTalkButton => micMode == MicMode.pushToTalk;

  int get activeCount => runtimes.length;
  /// Servers that hold a live session in the engine.
  ///
  /// Deliberately not `runtimes.length`. [runtimeFor] creates an entry on
  /// demand, and the server list builds one card per saved server, so merely
  /// drawing the screen used to make the engine look full: with three saved
  /// servers the count read three however many were registered, the limit
  /// checks below all failed, and nothing added or synced afterwards was ever
  /// registered at all. `runtimes` holds display state for every saved server;
  /// this holds the ones the engine knows about, which is what the limit is
  /// about.
  final Set<String> _registered = {};

  bool get canAddMore => _registered.length < maxServers;

  ServerRuntime runtimeFor(String id) =>
      runtimes.putIfAbsent(id, () => ServerRuntime());

  final _MeterNotifier _meters = _MeterNotifier();

  /// Fires when a level has moved and nothing else has.
  ///
  /// Anything that draws a meter, or is styled by whether somebody is
  /// currently speaking, should listen to this instead of to the state object
  /// as a whole — and should wrap only the part that actually changes, since
  /// this fires ten times a second whenever the microphone is open.
  ///
  /// Everything reachable from here still lives on [AppState]; this says when
  /// to look, not what to look at. A widget that reads a level without
  /// listening to this is not wrong, only slow to notice.
  Listenable get meters => _meters;

  /// Every user currently talking across all connected servers.
  List<String> get allSpeakingNames => [
    for (final rt in runtimes.values)
      if (rt.isLive) ...rt.speakingNames,
  ];

  Future<void> start() async {
    try {
      final prefs = await SharedPreferences.getInstance();
      _loadSettings(prefs);

      // Permission only. The session is taken live per call, not here — see
      // [_acquireAudio] — so that the recording indicator and the hands-free
      // Bluetooth profile belong to a conversation rather than to having the
      // app installed. A refusal is still worth catching now, because it is
      // the one answer that makes everything downstream pointless.
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
        onEvent,
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

      // Coming back to the app is the moment a stale list is most visible: the
      // other device was edited while this one sat in a pocket, and the first
      // thing anyone does is look at the list they expect to have changed.
      //
      // A suspended app receives no iCloud notification, and the one the system
      // posts on wake is not something to rely on — it does not arrive when the
      // store already held the value, and it has a reputation for not arriving
      // at all. Asking directly costs one read of a few hundred bytes.
      _lifecycle = AppLifecycleListener(
        onResume: () {
          unawaited(syncNow());
          // Belt and braces with [onShow]. Visibility is the signal that
          // matters here, but a platform that reports only resume must not be
          // left with a list that never refreshes again.
          _resumeProbing();
        },
        onShow: _resumeProbing,
        onHide: _pauseProbing,
      );

      // Not awaited: the window is worth having but nothing else waits on it,
      // and on Android it can fail for want of a permission the user has to
      // grant in system settings. A failure there leaves the setting off
      // without an alarm rather than blocking a startup that is otherwise fine.
      if (_wantOverlay && overlay.isSupported) {
        unawaited(enableOverlay());
      }

      _resumeProbing();

      _ready = true;
      // Last, and then only once the app has gone quiet — see [_probeWhenIdle].
      _probeWhenIdle();
    } catch (e) {
      _startupError = e.toString();
      // `_probeWhenIdle` is the last line of the `try` and a throw above it
      // skips the arming entirely, so nothing would ever resolve the spinner.
      // There is no chain to measure on this path and the screen already says
      // why, which makes the plain icon the honest one.
      _settleProbeUnmeasured();
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
    if (prefs.getInt(_prefsJitterBuffer) case final v?) {
      jitterBufferMs = _clampJitter(v);
    }
    reverb = prefs.getBool(_prefsReverb) ?? true;
    simpleModel = prefs.getBool(_prefsSimpleModel) ?? false;
    // Into the core immediately, and before the probe: it decides which model
    // every enhancer built afterwards loads, and the probe has to time the
    // arrangement that will really run rather than one the rider declined.
    setSimpleModel(on_: simpleModel);
    final guard = prefs.getInt(_prefsFeedbackGuard);
    if (guard != null &&
        guard >= 0 &&
        guard < FeedbackGuardMode.values.length) {
      feedbackGuard = FeedbackGuardMode.values[guard];
    }
    final hiss = prefs.getInt(_prefsDehiss);
    if (hiss != null && hiss >= 0 && hiss < DehissOption.values.length) {
      dehiss = DehissOption.values[hiss];
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
    setJitterBufferMs(ms: jitterBufferMs);
    setReverb(on_: reverb);
    setFeedbackGuard(mode: feedbackGuard);
    setDehiss(mode: dehiss);
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

    await _repairEscapedNames(prefs);

    for (final s in servers.take(maxServers)) {
      await _register(s);
    }
  }

  /// Decodes XML escapes left in names saved before the directory parser did.
  ///
  /// The parser is right and has been for a while — see [parsePublicList] —
  /// but a name it decoded is only decoded at the moment it is read. Entries
  /// added before that still hold what the directory sent, so a server called
  /// "Dordogne & Suisse" sits in the list as `Dordogne &amp; Suisse` and stays
  /// there. Worse now than it was: that name goes into the invite links and QR
  /// codes this device hands out, so one stale entry spreads.
  ///
  /// Once, guarded by a flag, rather than on every load. A rider who genuinely
  /// wants `&amp;` in a name they typed themselves is entitled to keep it, and
  /// a repair that ran forever would take it away every time.
  Future<void> _repairEscapedNames(SharedPreferences prefs) async {
    if (prefs.getBool(_prefsNamesRepaired) ?? false) return;

    var changed = false;
    for (var i = 0; i < servers.length; i++) {
      final name = servers[i].name;
      final decoded = _unescapeXml(name);
      if (decoded == name) continue;
      // Not stamped: this corrects how the name was always meant to read
      // rather than changing it, and stamping would have this device win a
      // sync against another that has the same entry spelled correctly.
      servers[i] = servers[i].copyWith(name: decoded);
      changed = true;
    }

    await prefs.setBool(_prefsNamesRepaired, true);
    if (changed) await _persist(publish: false);
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

  /// Drops a server's display state and its claim on an engine slot together.
  ///
  /// The two were separate and went out of step: the runtime was removed and
  /// the slot stayed taken, so a server swapped out never gave its place back.
  void _deregister(String id) {
    runtimes.remove(id);
    _registered.remove(id);
  }

  Future<void> _register(SavedServer s) async {
    try {
      await addServer(config: s.toConfig());
      _registered.add(s.id);
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
    if (_registered.length < maxServers) {
      await _register(entry);
    }
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  /// Whether a saved server may be edited or removed.
  ///
  /// Only while it is genuinely disconnected — idle, stopped, or given up.
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
      // Never registered, which is fine — _register puts it back either way.
    }
    _deregister(updated.id);
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
        fallbackUsername: fallbackUsername ?? await suggestedUsername(),
      );
      var added = 0;
      for (final c in configs) {
        final s = SavedServer.fromConfig(c);
        if (servers.any((e) => e.id == s.id)) continue;
        servers.add(s.stamped());
        if (_registered.length < maxServers) await _register(s);
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

  /// The name to use when an invitation carries none — which is now all of
  /// them, since a shared code no longer names the rider who shared it.
  ///
  /// A name already in use on this device wins, so a second server joins under
  /// the same name as the first and the rider is one person everywhere. Only a
  /// device with no servers at all asks the platform, and the answer is kept:
  /// when it falls through to a random pair, "amber-otter" must still be
  /// "amber-otter" tomorrow.
  Future<String> suggestedUsername() async {
    if (servers.isNotEmpty) return servers.first.username;

    final prefs = await SharedPreferences.getInstance();
    final kept = prefs.getString(_prefsSuggestedName);
    if (kept != null && kept.isNotEmpty) return kept;

    final suggestion = await DeviceIdentity.instance.suggest();
    await prefs.setString(_prefsSuggestedName, suggestion);
    return suggestion;
  }

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
    if (_registered.length < maxServers) await _register(copy);
    await _persist();
    notifyListeners();
    unawaited(refreshPings());
    return null;
  }

  // --- syncing between the user's devices -----------------------------------

  /// Whether to use the platform's sync facility, where there is one.
  bool cloudSync = true;

  /// Whether that facility is actually usable — signed in, and switched on for
  /// this app. Distinct from [cloudSync]: the user can want this and still not
  /// have it, and being told which is which is the difference between a
  /// setting that looks broken and one that explains itself.
  bool cloudReady = false;

  /// What went wrong last time, if anything.
  String? cloudError;

  /// Deletions, kept so they can outlive the entry and reach other devices.
  final Map<String, int> _deleted = {};

  Timer? _syncTimer;

  /// Re-reads iCloud when the app is brought back to the front. See where it
  /// is created for why the system's own notification is not enough.
  AppLifecycleListener? _lifecycle;
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
  /// same operation seen from either end — merge, keep what came of it, and
  /// publish it if it differs from what was there — and splitting them into a
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
    'jitterBufferMs': jitterBufferMs,
    'reverb': reverb,
    'feedbackGuard': feedbackGuard.index,
    'dehiss': dehiss.index,
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
    if (read<int>('jitterBufferMs') case final v?
        when _clampJitter(v) != jitterBufferMs) {
      jitterBufferMs = _clampJitter(v);
      await prefs.setInt(_prefsJitterBuffer, jitterBufferMs);
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
    if (read<int>('dehiss') case final v?
        when v >= 0 &&
            v < DehissOption.values.length &&
            DehissOption.values[v] != dehiss) {
      dehiss = DehissOption.values[v];
      await prefs.setInt(_prefsDehiss, v);
      setDehiss(mode: dehiss);
      changed = true;
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
  /// business — see `shared/CloudStore.swift`. All that matters here is that a
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
      _deregister(id);
      try {
        await removeServer(serverId: id);
      } catch (_) {
        // Never registered — only the first few entries ever are.
      }
    }

    for (final s in next.take(maxServers)) {
      final old = before[s.id];
      if (old == null) {
        if (_registered.length < maxServers) await _register(s);
        continue;
      }
      // A renamed server keeps talking. Connection details are baked into the
      // session when it is registered, so only those warrant a rebuild.
      if (old.sameConnection(s)) continue;

      // But never mid-call. A conversation is not worth interrupting for a
      // detail somebody altered on a laptop, and the change is not urgent —
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
      _deregister(s.id);
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
    _deregister(id);
    try {
      await removeServer(serverId: id);
    } catch (_) {}
    await _persist();
    notifyListeners();
    return null;
  }

  // --- microphone and speaker -------------------------------------------

  /// How long the devices stay open after the last call ends.
  ///
  /// Long enough that leaving one server and joining another, or a reconnect
  /// landing, does not close and reopen them. On Bluetooth that matters more
  /// than the battery it costs: each reopen renegotiates an SCO link, which
  /// takes a second or two and is audible in the helmet.
  static const _audioIdleGrace = Duration(seconds: 10);

  Timer? _audioRelease;
  bool _audioActive = false;

  /// Whether the microphone and speaker are open.
  ///
  /// The interface asks so it can say what is going on rather than drawing a
  /// dead level meter, which reads as a microphone that has broken.
  bool get audioActive => _audioActive;

  /// Opens the devices for a call. Returns why it could not, or null.
  ///
  /// Awaited from [connect] rather than deferred to the first press of the
  /// talk button. Opening a Bluetooth headset means negotiating an SCO link:
  /// one to two seconds, audible, and impossible to hide — so it goes where
  /// there is already a wait, behind a connect that takes about as long. A
  /// rider who presses talk and loses the first half of their sentence would
  /// have no idea why.
  Future<String?> _acquireAudio() async {
    _audioRelease?.cancel();
    _audioRelease = null;
    if (_audioActive) return null;

    // The platform session first: on iOS there is nothing for the engine to
    // open until the category is live, and the failure it produces otherwise
    // is CoreAudio's, which describes a channel count rather than the phone
    // call that is holding the microphone.
    final session = await AudioSessionBridge.instance.activate();
    if (!session.usable) {
      // The platform's own wording when there is one — it names the app that
      // has the microphone, which is the only version anybody can act on —
      // and ours, translated, when there is not.
      return session.error ?? _strings.micUnavailable;
    }

    try {
      await setAudioActive(on_: true);
    } catch (e) {
      // Never leave the session live with no engine behind it: that is the
      // recording indicator on, for nothing.
      await AudioSessionBridge.instance.deactivate();
      return e.toString();
    }

    _audioActive = true;
    _syncClassifier();
    _syncReliefWatch();
    notifyListeners();
    return null;
  }

  /// Closes the devices once nothing has wanted them for a while.
  void _releaseAudioSoon() {
    if (!_audioActive || _audioRelease != null) return;
    _audioRelease = Timer(_audioIdleGrace, () async {
      _audioRelease = null;
      // Re-checked rather than assumed: ten seconds is long enough for a
      // reconnect to land, and closing the devices under a live call would be
      // a conversation going silent for no reason anybody could see.
      if (_audioNeeded || !_audioActive) return;
      try {
        await setAudioActive(on_: false);
      } catch (_) {
        // Already shut, or the engine has gone. Either way there is nothing
        // holding the devices that matters.
      }
      await AudioSessionBridge.instance.deactivate();
      _audioActive = false;
      _syncClassifier();
    _syncReliefWatch();
      notifyListeners();
    });
  }

  /// Screens that want the devices open for as long as they are on show.
  ///
  /// A count rather than a flag: the audio settings can be reached from more
  /// than one route, and two screens releasing a single flag would shut the
  /// microphone under whichever was still open.
  int _audioHolds = 0;

  /// Opens the devices and keeps them open until [releaseAudio].
  ///
  /// For the audio settings, where every control is about a signal the user
  /// needs to hear or see the effect of. Waiting for them to switch on the
  /// microphone test first would mean the meter above the gain slider — the
  /// one thing that says whether a change helped — is dead at the moment it
  /// matters most.
  Future<String?> holdAudio() async {
    _audioHolds++;
    final error = await _acquireAudio();
    if (error != null) {
      // Given back through the same door it was taken from, rather than by
      // decrementing here. A screen closed while the microphone was still
      // answering has already released this hold, and a second bare `--`
      // would take the count below zero — where the next screen to ask for
      // audio would raise it only to nought and be quietly ignored.
      releaseAudio();
    }
    return error;
  }

  void releaseAudio() {
    if (_audioHolds > 0) _audioHolds--;
    _syncAudioToUse();
  }

  /// How many things are currently keeping the devices open.
  ///
  /// Exposed for tests because both ways of getting this wrong are silent. A
  /// hold taken and never returned leaves the microphone open for the rest of
  /// the session — the recording indicator lit, the battery going, and nothing
  /// on screen to say why. A hold returned twice drops somebody else's, and the
  /// microphone shuts under a screen that is still using it.
  @visibleForTesting
  int get audioHolds => _audioHolds;

  /// Everything that needs the devices open, which is not only calls.
  ///
  /// The microphone test in settings is a rider holding a headset and
  /// listening to themselves through it. That needs the devices exactly as
  /// much as a conversation does, and leaving it out would have turned a
  /// switch that works into one that silently does nothing.
  bool get _audioNeeded => _callInProgress || monitoring || _audioHolds > 0;

  /// Whether a diagnostic recording is running.
  ///
  /// Held here rather than read from the engine because it owns an audio hold,
  /// and a hold has to be given back exactly once by whoever took it.
  bool get diagnosticRecording => _diagnosticRecording;
  bool _diagnosticRecording = false;

  /// Starts a diagnostic recording, with the devices open behind it.
  ///
  /// **The hold is the point of this method.** The recorder is fed by the
  /// capture worker, and the worker does not run until the engine has opened
  /// the devices. Calling the engine's recorder without one produces a file
  /// that is empty, valid, and indistinguishable from a ride nobody spoke on —
  /// which is precisely the class of silent failure this whole feature exists
  /// to remove.
  ///
  /// Taking the hold is also what puts the platform in the right state, and
  /// that matters twice over on Android: without `MODE_IN_COMMUNICATION` the
  /// hands-free link is never established and the recording is made from the
  /// *phone's* microphone. That is the exact confusion that invalidated every
  /// measurement made before this existed, so a recorder that could reproduce
  /// it would be worse than none.
  ///
  /// Returns null on success, or something to show the rider. Muting does not
  /// stop it: mute is applied at the transmit decision, well after the block is
  /// handed to the recorder, so a rider can record a ride without sending any
  /// of it — which is usually what they want.
  Future<String?> beginDiagnosticRecording(String directory, String tag) async {
    if (_diagnosticRecording) return null;

    // Awaited before the recorder is told anything. If the microphone cannot
    // be had — refused, or held by another app — the honest outcome is a
    // switch that refuses to move and says why, not a file of silence.
    final error = await holdAudio();
    if (error != null) return error;

    try {
      startDiagnosticRecording(directory: directory, tag: tag);
    } catch (e) {
      releaseAudio();
      return e.toString();
    }
    _diagnosticRecording = true;
    notifyListeners();
    return null;
  }

  /// Stops the recording, closes the files and gives the devices back.
  ///
  /// Returns the blocks storage could not keep up with. Safe to call when
  /// nothing is recording, which is what teardown does.
  int endDiagnosticRecording() {
    if (!_diagnosticRecording) return 0;
    _diagnosticRecording = false;
    int dropped = 0;
    try {
      dropped = stopDiagnosticRecording().toInt();
    } catch (_) {
      // The engine has gone. The hold below still has to be returned.
    }
    releaseAudio();
    notifyListeners();
    return dropped;
  }

  /// Keeps the devices following whatever is using them.
  ///
  /// Only ever releases. Acquiring is done by the things that need them —
  /// [connect], [toggleMonitoring], [testOutput] — because each has somewhere
  /// to report a refusal to and something to abandon if the microphone cannot
  /// be had. This runs from an event handler, where both would be lost.
  void _syncAudioToUse() {
    if (_audioNeeded) {
      _audioRelease?.cancel();
      _audioRelease = null;
      return;
    }
    _releaseAudioSoon();
  }

  /// Moves the server the rider just chose up the list, under any that are
  /// already live.
  ///
  /// **Not simply "newest to the top".** This app holds more than one server
  /// at a time, and a plain most-recently-used list would push a live
  /// conversation down a place every time a second one was joined — the list
  /// reordering under a thumb that is reaching for it. So a server that is
  /// already connected keeps its place, and the one being connected lands
  /// directly beneath it.
  ///
  /// Pure, and taking the liveness test as an argument, so the ordering can be
  /// tested without an engine behind it.
  @visibleForTesting
  static void promoteOnConnect(
    List<SavedServer> servers,
    String id,
    bool Function(String id) isLive,
  ) {
    final from = servers.indexWhere((s) => s.id == id);
    if (from < 0) return;
    final entry = servers.removeAt(from);
    // The run of already-live servers at the top, counted after the removal so
    // reconnecting the one already at the front is a no-op rather than a
    // shuffle.
    var to = 0;
    while (to < servers.length && isLive(servers[to].id)) {
      to++;
    }
    servers.insert(to, entry);
  }

  Future<void> connect(String id) async {
    // Ordered on the rider's choice rather than on the handshake succeeding.
    // A server that fails to answer is still the one they reached for, and a
    // list that reorders only on success would leave the entry they just
    // tapped somewhere below the one they did not.
    final before = [for (final s in servers) s.id];
    promoteOnConnect(servers, id, (i) => runtimes[i]?.isLive ?? false);
    if (!listEquals(before, [for (final s in servers) s.id])) {
      unawaited(_persist());
      notifyListeners();
    }

    // Details that arrived from another device while this session was in use
    // are applied now, on the way in, rather than having interrupted it then.
    if (_pendingRegistration.remove(id)) {
      final i = servers.indexWhere((s) => s.id == id);
      if (i >= 0) {
        try {
          await removeServer(serverId: id);
        } catch (_) {}
        _deregister(id);
        await _register(servers[i]);
      }
    }
    // A server past the limit holds no engine slot until somebody wants it.
    // Slots are surrendered by whichever registered server is not in a call,
    // so the ceiling applies to conversations rather than to position in the
    // list — without this the third entry could never be reached however idle
    // the first two were, which is exactly how it read: a note about servers
    // "connected at once" on a screen where nothing was connected at all.
    if (!_registered.contains(id)) {
      final error = await _claimSlotFor(id);
      if (error != null) {
        runtimeFor(id)
          ..status = ConnStatus.failed
          ..detail = error;
        notifyListeners();
        return;
      }
    }

    // Before the handshake, and awaited. There is no point joining a channel
    // this device cannot speak or listen on, and the reason a microphone
    // could not be opened — another app holding it, a headset that has gone
    // — is something the rider can act on only if they are told.
    final audioError = await _acquireAudio();
    if (audioError != null) {
      runtimeFor(id)
        ..status = ConnStatus.failed
        ..detail = audioError;
      notifyListeners();
      return;
    }

    try {
      await connectServer(serverId: id);
    } catch (e) {
      runtimeFor(id)
        ..status = ConnStatus.failed
        ..detail = e.toString();
      notifyListeners();
      // Nothing came of it, so the devices go back unless something else is
      // using them.
      _syncAudioToUse();
    }
  }

  /// Registers [id] with the engine, freeing a slot first if they are all
  /// taken. Returns why it could not be done, or null.
  ///
  /// Only a genuinely disconnected server is ever displaced — the same test
  /// that decides whether one may be edited. Evicting a server mid-call to
  /// make room for another would be an unannounced drop in a conversation,
  /// which is the one thing this must not do to a rider.
  Future<String?> _claimSlotFor(String id) async {
    final index = servers.indexWhere((s) => s.id == id);
    if (index < 0) return 'That server is no longer in your list.';

    if (_registered.length >= maxServers) {
      final spare = _registered
          .where((other) => runtimeFor(other).isModifiable)
          .firstOrNull;
      if (spare == null) return _strings.allSlotsInUse(maxServers);

      try {
        await removeServer(serverId: spare);
      } catch (_) {
        // Already gone as far as the engine is concerned; the slot is still
        // ours to reclaim either way.
      }
      _deregister(spare);
    }

    await _register(servers[index]);
    if (_registered.contains(id)) return null;
    // _register swallows the failure into the runtime's detail, which is where
    // the reason actually is.
    final detail = runtimeFor(id).detail;
    return detail.isEmpty ? 'That server could not be prepared.' : detail;
  }

  Future<void> disconnect(String id) async {
    try {
      await disconnectServer(serverId: id);
    } catch (_) {}
  }

  /// Closes every live connection, for an app that is going away.
  ///
  /// Mumble has no goodbye message: a client leaves by closing its socket, and
  /// the other riders find out because the server notices. Left to itself that
  /// happens whenever the process finally dies, which on a phone can be minutes
  /// after the rider thinks they have closed the app — and for all of those
  /// minutes they are on everyone else's list, present and silent.
  ///
  /// Best effort by nature. It runs on the way out, when there may be very
  /// little time left, so every connection is asked at once rather than in
  /// turn and none of them is waited on for long.
  Future<void> disconnectAll() async {
    final ids = _registered.toList();
    if (ids.isEmpty) return;
    await Future.wait(ids.map(disconnect)).timeout(
      const Duration(seconds: 2),
      // A socket that will not close politely still closes when the process
      // goes. Waiting longer for it only delays that.
      onTimeout: () => const [],
    );
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
      // prefer — so the trust the user just granted gets rolled back.
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

  /// Starts re-probing saved servers, and probes once straight away.
  ///
  /// Idempotent: [AppLifecycleListener] reports both showing and resuming on
  /// the way back, and doing this twice would fire two rounds of probes at the
  /// exact moment the interface is being rebuilt.
  void _resumeProbing() {
    if (_pingTimer != null) return;
    _pingTimer = Timer.periodic(_pingInterval, (_) => refreshPings());
    // Immediately, not in fifteen seconds. Coming back to the app is when a
    // stale reading is most visible, and the timer's first tick is a whole
    // interval away.
    unawaited(refreshPings());
  }

  /// Stops probing while the app is not on screen.
  ///
  /// Each round opens a UDP socket per saved server and waits up to three
  /// seconds for a reply. On a phone the packet is not the cost — waking the
  /// radio is, every fifteen seconds, for a list nobody is looking at. On a
  /// bike that is the normal state: the app is behind a map for the whole ride.
  ///
  /// This cannot affect a live connection. The probe is an anonymous status
  /// query on its own socket ([`ping_server`]); a session's keepalive and the
  /// silence timeout that detects a dropped link both run inside the engine,
  /// on their own timers, and never consult this. What stops here is the
  /// reachability line on the server cards, which is a thing you read, and
  /// there is nobody reading it.
  void _pauseProbing() {
    _pingTimer?.cancel();
    _pingTimer = null;
  }

  /// Re-probes every saved server. Offline servers simply report unreachable.
  ///
  /// Connected servers are probed too. The reachability line is shown on every
  /// card whether or not it is joined, and the occupancy in it — how many
  /// people are on the server, as against in your channel — has no other
  /// source. Skipping them would save one packet per server and freeze a line
  /// the rider can see.
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

  /// Moves to another channel. Returns why not, when it fails.
  ///
  /// The failure used to be swallowed whole, which made a refused join
  /// indistinguishable from a dead button: the tap did nothing, said nothing,
  /// and left no trace anywhere. A server can decline for reasons the app
  /// cannot see — a full channel, one that needs a password, or a permission
  /// the account does not have — so the reason has to come back out.
  Future<String?> joinChannelOn(String id, int channelId) async {
    try {
      await joinChannel(serverId: id, channelId: channelId);
      return null;
    } catch (e) {
      return e.toString();
    }
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

  /// Loopback: hear your own microphone. Returns why it could not start.
  ///
  /// Opens the devices first, and holds them for as long as it is on. This is
  /// the one place in the app where somebody wants the microphone without
  /// wanting a conversation, and the whole point of the test is hearing
  /// something — a switch that turned itself on over a shut microphone would
  /// be a worse answer than the refusal.
  Future<String?> toggleMonitoring() async {
    if (!monitoring) {
      final error = await _acquireAudio();
      if (error != null) return error;
    }
    monitoring = !monitoring;
    setMonitoring(on_: monitoring);
    notifyListeners();
    // Hands the devices back when it is switched off, unless a call has them.
    _syncAudioToUse();
    return null;
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

  /// Runs the light speech-enhancement model instead of the full one.
  ///
  /// Off by default: on a rider talking normally in still air the full model
  /// is the better of the two, and the light one takes 4 to 6 dB more out of
  /// the speech. It is here for phones that cannot afford the full one, where
  /// the alternative is the performance ladder taking the rest of the chain
  /// apart instead — see `core/src/audio/deepfilter.rs`.
  bool simpleModel = false;

  Future<void> setSimpleModelEnabled({required bool value}) async {
    simpleModel = value;
    // Takes effect on the running chain within a block, and on every enhancer
    // built afterwards. Changing it mid-call rebuilds the model once, which is
    // the cost of a deliberate action rather than something happening to a
    // rider unasked.
    setSimpleModel(on_: value);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setBool(_prefsSimpleModel, value);
  }

  /// Runs the model, or does not. Owned here because it has to keep working
  /// with every screen closed and the phone in a pocket, which is most of a
  /// ride.
  final classifier = BackgroundClassifier();

  Timer? _reliefTimer;
  bool _chainDegraded = false;
  bool _probed = false;
  bool _probeAnswered = false;
  Timer? _probeTimer;

  /// Whether the toolbar still has nothing true to say about this device.
  ///
  /// The toolbar shows a spinner in place of the diagnostics icon while this
  /// is true: that icon is about to claim either "fine" or "degraded", and
  /// neither has been decided yet.
  ///
  /// **From launch, not from the start of the measurement.** This used to be
  /// "is the probe running", which left the plain icon on screen for the whole
  /// of startup and the five-second settle after it — several seconds of
  /// claiming the chain was fine, before anything had looked. That is the
  /// state the spinner exists to avoid, and it was the longer half of it.
  ///
  /// It resolves on every path that can produce an answer, which is what makes
  /// it safe to start true:
  ///
  ///   * the probe finishes, or fails, or finds no engine — [_probeChain];
  ///   * the devices open before it ever ran, so the ladder is now measuring
  ///     the real chain every block and is the better authority anyway —
  ///     [_syncReliefWatch];
  ///   * startup failed, so there is no chain to measure and the screen has an
  ///     error on it — [_init].
  ///
  /// A spinner that never stops is worse than an icon that is briefly wrong,
  /// so a path that resolves this must not be removed without another taking
  /// its place. `probe_spinner_test.dart` holds each of them.
  bool get probing => !_probeAnswered;

  /// How long the app is left alone before it is measured.
  ///
  /// **The probe competes with whatever else is running, and it cannot tell the
  /// difference.** It times the chain against a wall clock, so any CPU the app
  /// is still spending on its own startup — the engine opening, the server list
  /// loading, iCloud syncing, the first frames rasterising — is charged to the
  /// chain and dials the rider down a rung they did not need to lose. Startup
  /// is exactly when a phone is busiest, which makes the obvious moment to
  /// measure the worst one.
  static const Duration _probeSettle = Duration(seconds: 5);

  /// Measures once the app has finished opening and nothing else is running.
  ///
  /// Two conditions, and the second one is why this is a method rather than a
  /// delay: the devices must be shut. A call in progress means the real capture
  /// chain is already running every 10 ms, and measuring a second copy of it
  /// against a wall clock would report roughly double. So a probe that arrives
  /// during a call is dropped and retried when the devices close — see
  /// [_syncReliefWatch], which is called on both transitions.
  void _probeWhenIdle() {
    if (_probed || _probeTimer != null) return;
    _probeTimer = Timer(_probeSettle, () {
      _probeTimer = null;
      unawaited(_probeChain());
    });
  }

  /// Measures the chain against the block deadline and dials the ladder.
  ///
  /// **Off the platform thread**, which is what the non-`sync` bridge call
  /// buys: it loads the model and runs several hundred blocks, and doing that
  /// on the UI thread would freeze the app.
  ///
  /// Failure is not an error worth showing. A device that cannot even be
  /// measured gets the behaviour it had before this existed: the ladder starts
  /// at the top and steps down if it has to.
  ///
  /// The numbers behind the decision are not kept here — the Rust side writes
  /// them into the app's own log as it finishes, which is where a rider can
  /// read them back and quote them. Holding a second copy in the state would be
  /// a field with no reader.
  Future<void> _probeChain() async {
    if (_probed) return;
    if (_audioActive) {
      // Not now, and not never: the devices closing re-arms this.
      return;
    }
    _probed = true;
    notifyListeners();
    try {
      if ((await audioProbeChain()).relief > 0) {
        // The warning icon, before the first call rather than during it.
        _chainDegraded = true;
      }
    } catch (_) {
      // No measurement. The runtime ladder is unaffected and still authoritative.
    } finally {
      // On every path out, or the toolbar spins for the rest of the session
      // over a measurement that already failed.
      _probeAnswered = true;
      notifyListeners();
    }
  }

  /// Stops the spinner without an answer from the probe.
  ///
  /// Used where waiting longer cannot produce one: the devices opened first,
  /// or startup failed outright. Named rather than inlined because the count
  /// of these paths is the thing that keeps the spinner from being permanent.
  void _settleProbeUnmeasured() {
    if (_probeAnswered) return;
    _probeAnswered = true;
    notifyListeners();
  }

  /// Whether the performance ladder has switched any capture stage off.
  ///
  /// **Watched outside the diagnostics panel on purpose.** The panel is where
  /// the detail lives, and a rider whose voice has quietly got worse has no
  /// reason to open it — so the toolbar icon has to be able to say that
  /// something is wrong before anyone goes looking. It is a `#[frb(sync)]`
  /// read of state the worker publishes every block anyway, so watching it
  /// costs a comparison every two seconds.
  bool get chainDegraded => _chainDegraded;

  /// Polls only while the devices are open, because the ladder can only step
  /// while blocks are being processed.
  void _syncReliefWatch() {
    if (_audioActive) {
      _reliefTimer ??= Timer.periodic(
        const Duration(seconds: 2),
        (_) => _pollRelief(),
      );
      _pollRelief();
      // A call beat the probe to it. The real chain is now being measured
      // every block by the ladder itself, which is a better authority than a
      // synthetic run — and a probe deferred until the devices shut could be
      // twenty minutes away. Spinning for the length of a ride would read as
      // broken, so the icon starts speaking for the ladder here.
      _settleProbeUnmeasured();
      return;
    }
    _reliefTimer?.cancel();
    _reliefTimer = null;
    // The devices just closed, which is the condition the probe was waiting
    // for if a call beat it to the start. Costs nothing when it has already
    // run — see [_probeWhenIdle].
    _probeWhenIdle();
    // Deliberately **not** clearing the flag. What was given up is a fact
    // about this device for the rest of the session — the ladder never climbs
    // back — so clearing it when a call ends would hide the warning at exactly
    // the moment a rider stops talking and goes looking for why they sounded
    // wrong.
  }

  /// Whether the ladder has given up the per-participant volume meters.
  ///
  /// Read by the channel list rather than by the diagnostics panel, because
  /// this is the one rung a rider sees without opening that panel. Polled on
  /// the same two-second timer as [chainDegraded]; a meter that keeps moving
  /// for two more seconds after the rung goes costs nothing, and polling it
  /// per frame would be the thing this rung exists to avoid.
  bool get participantMetersDisabled => _participantMetersDisabled;
  bool _participantMetersDisabled = false;

  void _pollRelief() {
    final bool degraded;
    try {
      final status = audioChainStatus();
      degraded = status.relief > 0;
      // Set and cleared, unlike the warning flag below: this one describes
      // what is running *now* rather than what this device turned out to be,
      // and a meter that never came back after a rebuilt engine would look
      // like a bug in the channel list.
      if (status.participantMetersDisabled != _participantMetersDisabled) {
        _participantMetersDisabled = status.participantMetersDisabled;
        notifyListeners();
      }
    } catch (_) {
      // No engine. Nothing has been given up that this can know about.
      return;
    }
    // **Only ever set, never cleared.** The ladder does not climb back, and
    // the startup probe can raise this before an engine exists at all — so a
    // poll that found a fresh engine at rung 0 would otherwise wipe a warning
    // that is still true. The same reasoning as `_syncReliefWatch`, which is
    // why the flag survives a call ending.
    if (!degraded || _chainDegraded) return;
    _chainDegraded = true;
    notifyListeners();
  }

  /// Starts or stops the classifier to match the current conditions.
  ///
  /// Four things have to be true at once, and any of them can change at any
  /// time: the devices are open (there is nothing to listen to otherwise),
  /// `Auto` is chosen (nothing else reads the verdict), the rider has left the
  /// switch on, and the platform can run the model at all.
  void _syncClassifier() {
    final want =
        _audioActive &&
        noise == NoiseSetting.auto &&
        BackgroundClassifier.supportedHere;
    if (want == classifier.running) return;
    if (want) {
      unawaited(classifier.start());
    } else {
      unawaited(classifier.stop());
    }
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

  /// The bounds the engine will actually honour, in milliseconds, as
  /// `(minimum, maximum, step)`.
  ///
  /// Asked of the engine rather than repeated here. Voice arrives in 20 ms
  /// packets and the buffer counts in whole ones, so a slider free to pick 95
  /// would be showing a number the engine had already rounded away.
  /// The fallback is the same arithmetic, for a widget test that never brought
  /// the engine up: the bounds are constants either side of the bridge, and a
  /// settings screen that cannot be built without a running audio engine would
  /// be untestable for no gain.
  static final (int, int, int) jitterBounds = () {
    try {
      return jitterBufferBoundsMs();
    } catch (_) {
      return (40, 500, 20);
    }
  }();

  /// How much incoming audio is held back before it is played, in ms.
  ///
  /// The floor, not the whole story: the engine still deepens the buffer by
  /// itself when a link starts losing packets, and comes back down to this.
  int jitterBufferMs = 200;

  static int _clampJitter(int ms) {
    final (lo, hi, step) = jitterBounds;
    final snapped = ((ms + step ~/ 2) ~/ step) * step;
    return snapped.clamp(lo, hi);
  }

  Future<void> setJitterBuffer({required int ms}) async {
    final value = _clampJitter(ms);
    if (value == jitterBufferMs) return;
    jitterBufferMs = value;
    setJitterBufferMs(ms: value);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_prefsJitterBuffer, value);
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
  /// one of these costs something — half duplex, a hard cut, or a quieter
  /// talker — which is not worth paying until there is a fault to fix.
  FeedbackGuardMode feedbackGuard = FeedbackGuardMode.off;

  /// Whether the microphone is actually reaching a server right now.
  ///
  /// Not the same as the talk button being held. In the hands-free modes
  /// nobody holds anything and the microphone opens by itself, and in every
  /// mode a muted microphone or no connection means nothing leaves this device
  /// however hard the button is pressed. One definition, because the floating
  /// window's on-air light and the meter on the main screen disagreeing about
  /// whether a rider is being heard would be worse than either being wrong.
  bool get isOnAir {
    final connected = runtimes.values.any((r) => r.isLive);
    return switch (micMode) {
      MicMode.pushToTalk => _transmitting,
      MicMode.voiceActivity => connected && !_muted && _speaking,
      MicMode.continuous => connected && !_muted,
    };
  }

  /// How to deal with the steady hiss a microphone adds under speech.
  ///
  /// Off by default. Both of the others discard something real, and a link that
  /// is already carrying a voice is not worth degrading until there is a fault
  /// to fix.
  DehissOption dehiss = DehissOption.off;

  Future<void> updateDehiss(DehissOption mode) async {
    dehiss = mode;
    setDehiss(mode: mode);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_prefsDehiss, mode.index);
  }

  Future<void> updateFeedbackGuard(FeedbackGuardMode mode) async {
    feedbackGuard = mode;
    setFeedbackGuard(mode: mode);
    notifyListeners();
    final prefs = await SharedPreferences.getInstance();
    await prefs.setInt(_prefsFeedbackGuard, mode.index);
  }

  /// Plays a short tone through the speaker. Returns why it could not.
  ///
  /// Needs the devices as much as a call does — this is somebody checking
  /// which end of a headset is which — so it opens them and lets the idle
  /// timer hand them back. The grace period outlasts the tone by some way,
  /// which also means pressing the button twice does not close and reopen a
  /// Bluetooth link in between.
  Future<String?> testOutput() async {
    final error = await _acquireAudio();
    if (error != null) return error;
    playTestTone(millis: 700);
    _syncAudioToUse();
    return null;
  }

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
  /// again, so it defaults to present rather than to absent — and having to
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
      // longer want it — and having the switch flip itself off every time they
      // looked at the app made it read as broken.
      notifyListeners();
    };
    overlay.onStatus = (message) {
      overlayStatus = message;
      notifyListeners();
    };

    overlayStatus = null;
    await overlay.setPhrases(_overlayPhrases());
    unawaited(_rememberOverlayChoice(true));

    // Turning the setting on is not the same as putting a window on screen.
    // With nothing connected there is no call for it to be about, so it stays
    // armed and invisible until one starts. See [_syncOverlayToCalls].
    if (!_anyServerLive) {
      overlayEnabled = false;
      notifyListeners();
      return null;
    }

    final error = await overlay.show();
    overlayEnabled = error == null;
    notifyListeners();
    _lastOverlaySignature = '';
    _pushOverlay();
    return error;
  }

  bool get _anyServerLive => runtimes.values.any((r) => r.isLive);

  /// Whether the engine is holding a conversation, or chasing one.
  ///
  /// Wider than [_anyServerLive] on purpose. A session being dialled or fought
  /// back through a reconnect needs the processor exactly as much as a
  /// connected one — more, since it is doing the work — and it is precisely
  /// during a reconnect, with the phone in a pocket and the screen off, that
  /// letting the device suspend turns a recoverable drop into a lost call.
  bool get _callInProgress =>
      runtimes.values.any((r) => r.isLive || r.isBusy);

  /// Whether anything said right now would actually reach somebody.
  ///
  /// Stricter than [_callInProgress], which counts a reconnect as a call worth
  /// staying awake for. Nothing leaves the phone during a reconnect, so for the
  /// question "did that go out" only a live session counts.
  bool get anyLive => runtimes.values.any((r) => r.isLive);

  bool? _lastCallActive;

  /// Tells the platform whether there is a call worth staying awake for.
  ///
  /// Only on transitions. This is reached from [_pushOverlay], which runs on
  /// every roster and level update, and a method channel hop ten times a
  /// second to repeat an unchanged answer would be its own small drain.
  void _syncKeepAliveToCalls() {
    final active = _callInProgress;
    if (active == _lastCallActive) return;
    _lastCallActive = active;
    unawaited(PowerBridge.instance.setCallActive(active));
  }

  /// Guards the show/hide calls below. [_pushOverlay] runs on every roster and
  /// level update — ten times a second during a call — and each transition
  /// must be requested once rather than on every frame until it completes.
  bool _overlayBusy = false;

  /// Keeps the floating window following the call rather than the app.
  ///
  /// The window's whole purpose is to keep a conversation reachable while the
  /// rider is looking at something else. With nothing connected there is no
  /// conversation, and what is left is a control panel for a call that is not
  /// happening — sitting over the map, on a motorcycle, where it is least
  /// welcome and hardest to dismiss.
  ///
  /// So it is driven by whether any server is live, not by the setting alone.
  /// The setting still decides whether it may appear at all; this decides
  /// when. It arms itself again by itself the moment a server connects, which
  /// is why turning it off here does not touch the stored preference.
  void _syncOverlayToCalls() {
    if (!_wantOverlay || !overlay.isSupported || _overlayBusy) return;
    final wanted = _anyServerLive;
    if (wanted == overlayEnabled) return;

    _overlayBusy = true;
    unawaited(() async {
      try {
        if (wanted) {
          final error = await overlay.show();
          overlayEnabled = error == null;
          _lastOverlaySignature = '';
        } else {
          await overlay.hide();
          overlayEnabled = false;
        }
      } catch (_) {
        // A window that will not open is reported through onStatus already;
        // failing here must not stop the call state being pushed.
      } finally {
        _overlayBusy = false;
      }
      notifyListeners();
    }());
  }

  /// Why the floating window did not appear, or null.
  ///
  /// Separate from the error [enableOverlay] returns, because the interesting
  /// failures happen after it has already reported success — the window is
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
  /// nothing visible has changed — this runs on every roster update.
  void _pushOverlay() {
    // Both before the early return: the window has to be taken down when the
    // last server drops, and at that moment overlayEnabled is still true —
    // and the wake lock has to be released whether or not there was ever a
    // window, which for a rider who turned the island off there was not.
    _syncOverlayToCalls();
    _syncKeepAliveToCalls();
    _syncAudioToUse();
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
    final live = isOnAir;
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
    // same event — so a partial one replaced the list with a single person,
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
    // The classifier exists to inform `Auto` and has no say under the other
    // four, so it stops running the moment one of them is chosen. Not merely
    // ignored -- stopped, because an inference every two seconds is a real
    // cost and nobody should pay it for an answer that will not be read.
    _syncClassifier();
    _syncReliefWatch();
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
  ///   advertise gzip — which looks exactly like the service being down. Dart's
  ///   `HttpClient` sends it by default, and it is requested explicitly here so
  ///   that stays true if the client is ever swapped out.
  /// * **`version` is required** — without it the endpoint also returns 501.
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
        final parsed = parsePublicList(res.body);
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
  /// English, for logs and the engine log view. Not for the interface — see
  /// [proxyConfig], which lets the caller build a translated line instead.
  String get proxyDescription => SystemProxy.instance.config.description;

  /// The resolved configuration, so a screen can describe it in the rider's
  /// language. `ProxyConfig.description` is a plain service class with no
  /// `BuildContext`, so its four strings were English wherever they were shown
  /// — which on the settings screen was under the proxy switch, in Russian.
  ProxyConfig get proxyConfig => SystemProxy.instance.config;

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
  @visibleForTesting
  static List<PublicServer> parsePublicList(String xml) {
    final out = <PublicServer>[];
    final re = RegExp(r'<server\b([^>]*)/?>', caseSensitive: false);
    final attr = RegExp('([a-zA-Z_]+)\\s*=\\s*"([^"]*)"');

    for (final m in re.allMatches(xml)) {
      final attrs = <String, String>{};
      for (final a in attr.allMatches(m.group(1) ?? '')) {
        // Decoded here rather than at the point of display, so every attribute
        // is unescaped exactly once no matter which of them gets used later.
        attrs[a.group(1)!.toLowerCase()] = _unescapeXml(a.group(2)!);
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

  static final RegExp _xmlEntity = RegExp(
    r'&(?:#(\d+)|#[xX]([0-9a-fA-F]+)|(amp|lt|gt|quot|apos));',
  );

  static const Map<String, String> _namedEntities = {
    'amp': '&',
    'lt': '<',
    'gt': '>',
    'quot': '"',
    'apos': "'",
  };

  /// Decodes the character references an XML attribute value can hold.
  ///
  /// The directory is XML, so a server called `Dordogne & Suisse` arrives as
  /// `Dordogne &amp; Suisse` — correctly encoded, and displayed verbatim by
  /// anything that forgets to decode it.
  ///
  /// One pass over the string, not a chain of `replaceAll` calls. Decoding
  /// `&amp;` first and the others afterwards would turn the literal text
  /// `&amp;lt;` into `<`, which is a different name than the one the server
  /// published. A single pass cannot rewrite its own output, so the ordering
  /// problem does not arise.
  static String _unescapeXml(String value) {
    if (!value.contains('&')) return value;
    return value.replaceAllMapped(_xmlEntity, (m) {
      final named = m[3];
      if (named != null) return _namedEntities[named]!;

      final digits = m[1] ?? m[2]!;
      final code = int.tryParse(digits, radix: m[1] != null ? 10 : 16);
      // Unrepresentable, or half a surrogate pair: leave the reference as
      // written. One malformed name should cost that name its ampersands, not
      // throw and take the whole directory listing down with it.
      if (code == null ||
          code < 0x20 ||
          code > 0x10FFFF ||
          (code >= 0xD800 && code <= 0xDFFF)) {
        return m[0]!;
      }
      return String.fromCharCode(code);
    });
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

  /// Applies one event from the engine.
  ///
  /// Exposed for tests because which events notify whom is a decision that has
  /// to keep holding: routing a level report through the main notifier costs
  /// nothing visible and rebuilds the whole interface twenty times a second.
  @visibleForTesting
  void onEvent(AppEvent event) {
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
        // Returns rather than falling through to [notifyListeners]: see
        // [meters]. Nothing outside a meter has changed, and this arrives ten
        // times a second for as long as the microphone is open.
        _meters.moved();
        _pushOverlay();
        return;
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
        // showed — a full bar for someone who went quiet a minute ago.
        for (final entry in runtimes.entries) {
          entry.value.decayUnreported(reported[entry.key] ?? const <int>{});
        }
        _meters.moved();
        _pushOverlay();
        return;
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
      case AppEvent_Refused(:final serverId, :final reason, :final kind):
        // Straight out again, without touching the roster or the chat log --
        // nothing here changed, which is the whole point of a refusal.
        _refusals.add(ServerRefusal(
          serverId: serverId,
          reason: reason,
          kind: kind,
        ));
        return;
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
    // Before anything else: this flushes and closes the recording. A file left
    // open by a process going away is a truncated file, and the rider finds out
    // when they try to share it.
    endDiagnosticRecording();
    _syncTimer?.cancel();
    _pingTimer?.cancel();
    _reliefTimer?.cancel();
    _probeTimer?.cancel();
    _audioRelease?.cancel();
    _lifecycle?.dispose();
    _events?.cancel();
    _meters.dispose();
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
