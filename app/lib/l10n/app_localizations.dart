import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_localizations_en.dart';
import 'app_localizations_ru.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of L
/// returned by `L.of(context)`.
///
/// Applications need to include `L.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'l10n/app_localizations.dart';
///
/// return MaterialApp(
///   localizationsDelegates: L.localizationsDelegates,
///   supportedLocales: L.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the L.supportedLocales
/// property.
abstract class L {
  L(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static L of(BuildContext context) {
    return Localizations.of<L>(context, L)!;
  }

  static const LocalizationsDelegate<L> delegate = _LDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[
    Locale('en'),
    Locale('ru'),
  ];

  /// No description provided for @appTitle.
  ///
  /// In en, this message translates to:
  /// **'MumbleWay'**
  String get appTitle;

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @save.
  ///
  /// In en, this message translates to:
  /// **'Save'**
  String get save;

  /// No description provided for @add.
  ///
  /// In en, this message translates to:
  /// **'Add'**
  String get add;

  /// No description provided for @remove.
  ///
  /// In en, this message translates to:
  /// **'Remove'**
  String get remove;

  /// No description provided for @delete.
  ///
  /// In en, this message translates to:
  /// **'Delete'**
  String get delete;

  /// No description provided for @settings.
  ///
  /// In en, this message translates to:
  /// **'Settings'**
  String get settings;

  /// No description provided for @language.
  ///
  /// In en, this message translates to:
  /// **'Language'**
  String get language;

  /// No description provided for @deafen.
  ///
  /// In en, this message translates to:
  /// **'Deafen'**
  String get deafen;

  /// No description provided for @undeafen.
  ///
  /// In en, this message translates to:
  /// **'Undeafen'**
  String get undeafen;

  /// No description provided for @muteMicrophone.
  ///
  /// In en, this message translates to:
  /// **'Mute microphone'**
  String get muteMicrophone;

  /// No description provided for @unmuteMicrophone.
  ///
  /// In en, this message translates to:
  /// **'Unmute microphone'**
  String get unmuteMicrophone;

  /// No description provided for @exportServers.
  ///
  /// In en, this message translates to:
  /// **'Export servers…'**
  String get exportServers;

  /// No description provided for @importFromFile.
  ///
  /// In en, this message translates to:
  /// **'Import from file…'**
  String get importFromFile;

  /// No description provided for @noServersTitle.
  ///
  /// In en, this message translates to:
  /// **'No servers yet'**
  String get noServersTitle;

  /// No description provided for @noServersBody.
  ///
  /// In en, this message translates to:
  /// **'Add a Mumble server to start talking. You can stay connected to two at once.'**
  String get noServersBody;

  /// No description provided for @addServer.
  ///
  /// In en, this message translates to:
  /// **'Add server'**
  String get addServer;

  /// No description provided for @addAnotherServer.
  ///
  /// In en, this message translates to:
  /// **'Add another server'**
  String get addAnotherServer;

  /// No description provided for @maxServersNote.
  ///
  /// In en, this message translates to:
  /// **'Up to {count} servers can be connected at once; the rest stay saved.'**
  String maxServersNote(int count);

  /// No description provided for @allSlotsInUse.
  ///
  /// In en, this message translates to:
  /// **'Already talking on {count} servers. Leave one first.'**
  String allSlotsInUse(int count);

  /// No description provided for @micIdleWithTalkButton.
  ///
  /// In en, this message translates to:
  /// **'The talk button and microphone meter appear here once you connect to a server.'**
  String get micIdleWithTalkButton;

  /// No description provided for @micIdleMeterOnly.
  ///
  /// In en, this message translates to:
  /// **'The microphone meter appears here once you connect to a server.'**
  String get micIdleMeterOnly;

  /// No description provided for @micIdleWhy.
  ///
  /// In en, this message translates to:
  /// **'The microphone stays closed until then, so nothing is recorded and your headset keeps its sound quality for other apps.'**
  String get micIdleWhy;

  /// No description provided for @micUnavailable.
  ///
  /// In en, this message translates to:
  /// **'The microphone could not be opened. Another app may be using it.'**
  String get micUnavailable;

  /// No description provided for @notConnectedAny.
  ///
  /// In en, this message translates to:
  /// **'Not connected to any server'**
  String get notConnectedAny;

  /// No description provided for @talkingOnOne.
  ///
  /// In en, this message translates to:
  /// **'Talking on 1 server'**
  String get talkingOnOne;

  /// No description provided for @talkingOnMany.
  ///
  /// In en, this message translates to:
  /// **'Talking on {count} servers simultaneously'**
  String talkingOnMany(int count);

  /// No description provided for @audioFailedTitle.
  ///
  /// In en, this message translates to:
  /// **'Audio could not start'**
  String get audioFailedTitle;

  /// No description provided for @audioFailedBody.
  ///
  /// In en, this message translates to:
  /// **'MumbleWay needs a microphone. Check that one is connected and that permission is granted, then restart the app.'**
  String get audioFailedBody;

  /// No description provided for @statusConnected.
  ///
  /// In en, this message translates to:
  /// **'Connected'**
  String get statusConnected;

  /// No description provided for @statusConnecting.
  ///
  /// In en, this message translates to:
  /// **'Connecting'**
  String get statusConnecting;

  /// No description provided for @statusAuthenticating.
  ///
  /// In en, this message translates to:
  /// **'Authenticating'**
  String get statusAuthenticating;

  /// No description provided for @statusReconnecting.
  ///
  /// In en, this message translates to:
  /// **'Reconnecting'**
  String get statusReconnecting;

  /// No description provided for @statusError.
  ///
  /// In en, this message translates to:
  /// **'Error'**
  String get statusError;

  /// No description provided for @statusDisconnected.
  ///
  /// In en, this message translates to:
  /// **'Disconnected'**
  String get statusDisconnected;

  /// No description provided for @statusNotConnected.
  ///
  /// In en, this message translates to:
  /// **'Not connected'**
  String get statusNotConnected;

  /// No description provided for @pttHoldToTalk.
  ///
  /// In en, this message translates to:
  /// **'HOLD TO TALK'**
  String get pttHoldToTalk;

  /// No description provided for @pttTransmitting.
  ///
  /// In en, this message translates to:
  /// **'TRANSMITTING'**
  String get pttTransmitting;

  /// No description provided for @pttMicrophoneMuted.
  ///
  /// In en, this message translates to:
  /// **'MICROPHONE MUTED'**
  String get pttMicrophoneMuted;

  /// No description provided for @pttVoiceActivated.
  ///
  /// In en, this message translates to:
  /// **'VOICE ACTIVATED'**
  String get pttVoiceActivated;

  /// No description provided for @pttOpenMic.
  ///
  /// In en, this message translates to:
  /// **'OPEN MIC'**
  String get pttOpenMic;

  /// No description provided for @probeChecking.
  ///
  /// In en, this message translates to:
  /// **'Checking…'**
  String get probeChecking;

  /// No description provided for @probeNotResponding.
  ///
  /// In en, this message translates to:
  /// **'Not responding'**
  String get probeNotResponding;

  /// No description provided for @connect.
  ///
  /// In en, this message translates to:
  /// **'Connect'**
  String get connect;

  /// No description provided for @disconnect.
  ///
  /// In en, this message translates to:
  /// **'Disconnect'**
  String get disconnect;

  /// No description provided for @joining.
  ///
  /// In en, this message translates to:
  /// **'joining…'**
  String get joining;

  /// No description provided for @shareInviteLink.
  ///
  /// In en, this message translates to:
  /// **'Share invite link'**
  String get shareInviteLink;

  /// No description provided for @shareProfileFile.
  ///
  /// In en, this message translates to:
  /// **'Share profile file'**
  String get shareProfileFile;

  /// No description provided for @duplicate.
  ///
  /// In en, this message translates to:
  /// **'Duplicate'**
  String get duplicate;

  /// No description provided for @removeServerTitle.
  ///
  /// In en, this message translates to:
  /// **'Remove server?'**
  String get removeServerTitle;

  /// No description provided for @removeServerBody.
  ///
  /// In en, this message translates to:
  /// **'{name} will be removed from your list.'**
  String removeServerBody(String name);

  /// No description provided for @includePasswordTitle.
  ///
  /// In en, this message translates to:
  /// **'Include the password?'**
  String get includePasswordTitle;

  /// No description provided for @includePasswordBody.
  ///
  /// In en, this message translates to:
  /// **'Anyone who receives this can join without being asked for a password. It stays valid for as long as the password does, wherever the message ends up.'**
  String get includePasswordBody;

  /// No description provided for @withoutPassword.
  ///
  /// In en, this message translates to:
  /// **'Without password'**
  String get withoutPassword;

  /// No description provided for @includeIt.
  ///
  /// In en, this message translates to:
  /// **'Include it'**
  String get includeIt;

  /// No description provided for @certChangedTitle.
  ///
  /// In en, this message translates to:
  /// **'Server certificate changed'**
  String get certChangedTitle;

  /// No description provided for @certChangedBody.
  ///
  /// In en, this message translates to:
  /// **'This can mean the server was reinstalled — or that someone is impersonating it. Only continue if you expected this.'**
  String get certChangedBody;

  /// No description provided for @trustNewCertificate.
  ///
  /// In en, this message translates to:
  /// **'Trust the new certificate'**
  String get trustNewCertificate;

  /// No description provided for @reconnectingIn.
  ///
  /// In en, this message translates to:
  /// **'Connection lost. Retrying in {seconds}s (attempt {attempt}).'**
  String reconnectingIn(int seconds, int attempt);

  /// No description provided for @connectionLost.
  ///
  /// In en, this message translates to:
  /// **'Connection lost.'**
  String get connectionLost;

  /// No description provided for @retryingInSeconds.
  ///
  /// In en, this message translates to:
  /// **'Retrying in {seconds}s (attempt {attempt}).'**
  String retryingInSeconds(int seconds, int attempt);

  /// No description provided for @retryingNow.
  ///
  /// In en, this message translates to:
  /// **'Retrying now (attempt {attempt})…'**
  String retryingNow(int attempt);

  /// No description provided for @switchToLanguage.
  ///
  /// In en, this message translates to:
  /// **'Tap to switch to {name}'**
  String switchToLanguage(String name);

  /// No description provided for @more.
  ///
  /// In en, this message translates to:
  /// **'More'**
  String get more;

  /// No description provided for @edit.
  ///
  /// In en, this message translates to:
  /// **'Edit'**
  String get edit;

  /// No description provided for @editServer.
  ///
  /// In en, this message translates to:
  /// **'Edit server'**
  String get editServer;

  /// No description provided for @saveChanges.
  ///
  /// In en, this message translates to:
  /// **'Save changes'**
  String get saveChanges;

  /// No description provided for @savingChanges.
  ///
  /// In en, this message translates to:
  /// **'Saving…'**
  String get savingChanges;

  /// No description provided for @displayName.
  ///
  /// In en, this message translates to:
  /// **'Display name'**
  String get displayName;

  /// No description provided for @displayNameHint.
  ///
  /// In en, this message translates to:
  /// **'Sunday ride'**
  String get displayNameHint;

  /// No description provided for @displayNameMissing.
  ///
  /// In en, this message translates to:
  /// **'Give it a name'**
  String get displayNameMissing;

  /// No description provided for @serverAddress.
  ///
  /// In en, this message translates to:
  /// **'Server address'**
  String get serverAddress;

  /// No description provided for @serverAddressHint.
  ///
  /// In en, this message translates to:
  /// **'mumble.example.com'**
  String get serverAddressHint;

  /// No description provided for @serverAddressMissing.
  ///
  /// In en, this message translates to:
  /// **'Enter an address'**
  String get serverAddressMissing;

  /// No description provided for @port.
  ///
  /// In en, this message translates to:
  /// **'Port'**
  String get port;

  /// No description provided for @portOutOfRange.
  ///
  /// In en, this message translates to:
  /// **'Port 1-65535'**
  String get portOutOfRange;

  /// No description provided for @username.
  ///
  /// In en, this message translates to:
  /// **'Username'**
  String get username;

  /// No description provided for @usernameMissing.
  ///
  /// In en, this message translates to:
  /// **'Enter a username'**
  String get usernameMissing;

  /// No description provided for @passwordOptional.
  ///
  /// In en, this message translates to:
  /// **'Password (optional)'**
  String get passwordOptional;

  /// No description provided for @passwordHelp.
  ///
  /// In en, this message translates to:
  /// **'Only if the server requires one'**
  String get passwordHelp;

  /// No description provided for @addingServer.
  ///
  /// In en, this message translates to:
  /// **'Adding…'**
  String get addingServer;

  /// No description provided for @quickerWays.
  ///
  /// In en, this message translates to:
  /// **'Quicker ways to add a server'**
  String get quickerWays;

  /// No description provided for @browsePublic.
  ///
  /// In en, this message translates to:
  /// **'Browse public'**
  String get browsePublic;

  /// No description provided for @importLabel.
  ///
  /// In en, this message translates to:
  /// **'Import'**
  String get importLabel;

  /// No description provided for @publicServers.
  ///
  /// In en, this message translates to:
  /// **'Public servers'**
  String get publicServers;

  /// No description provided for @search.
  ///
  /// In en, this message translates to:
  /// **'Search'**
  String get search;

  /// No description provided for @reload.
  ///
  /// In en, this message translates to:
  /// **'Reload'**
  String get reload;

  /// No description provided for @addToMyServers.
  ///
  /// In en, this message translates to:
  /// **'Add to my servers'**
  String get addToMyServers;

  /// No description provided for @noServersMatchSearch.
  ///
  /// In en, this message translates to:
  /// **'No servers match that search.'**
  String get noServersMatchSearch;

  /// No description provided for @importServers.
  ///
  /// In en, this message translates to:
  /// **'Import servers'**
  String get importServers;

  /// No description provided for @addFromText.
  ///
  /// In en, this message translates to:
  /// **'Add from text'**
  String get addFromText;

  /// No description provided for @profileFileFormat.
  ///
  /// In en, this message translates to:
  /// **'Profile file format'**
  String get profileFileFormat;

  /// No description provided for @serversAdded.
  ///
  /// In en, this message translates to:
  /// **'Servers added'**
  String get serversAdded;

  /// No description provided for @audioDevices.
  ///
  /// In en, this message translates to:
  /// **'Audio devices'**
  String get audioDevices;

  /// No description provided for @levels.
  ///
  /// In en, this message translates to:
  /// **'Levels'**
  String get levels;

  /// No description provided for @network.
  ///
  /// In en, this message translates to:
  /// **'Network'**
  String get network;

  /// No description provided for @microphone.
  ///
  /// In en, this message translates to:
  /// **'Microphone'**
  String get microphone;

  /// No description provided for @speakers.
  ///
  /// In en, this message translates to:
  /// **'Speakers'**
  String get speakers;

  /// No description provided for @systemDefault.
  ///
  /// In en, this message translates to:
  /// **'System default'**
  String get systemDefault;

  /// No description provided for @detectedAutomatically.
  ///
  /// In en, this message translates to:
  /// **'Detected automatically'**
  String get detectedAutomatically;

  /// No description provided for @recheckDevices.
  ///
  /// In en, this message translates to:
  /// **'Re-check devices'**
  String get recheckDevices;

  /// No description provided for @testSpeakers.
  ///
  /// In en, this message translates to:
  /// **'Test speakers'**
  String get testSpeakers;

  /// No description provided for @play.
  ///
  /// In en, this message translates to:
  /// **'Play'**
  String get play;

  /// No description provided for @stop.
  ///
  /// In en, this message translates to:
  /// **'Stop'**
  String get stop;

  /// No description provided for @speakerVolume.
  ///
  /// In en, this message translates to:
  /// **'Speaker volume'**
  String get speakerVolume;

  /// No description provided for @inputGain.
  ///
  /// In en, this message translates to:
  /// **'Input gain'**
  String get inputGain;

  /// No description provided for @hearMyself.
  ///
  /// In en, this message translates to:
  /// **'Hear myself'**
  String get hearMyself;

  /// No description provided for @hearMyselfHelp.
  ///
  /// In en, this message translates to:
  /// **'Plays your processed voice back. Use headphones — on speakers it will feed back.'**
  String get hearMyselfHelp;

  /// No description provided for @useSystemProxy.
  ///
  /// In en, this message translates to:
  /// **'Use the system proxy'**
  String get useSystemProxy;

  /// No description provided for @overrideProxy.
  ///
  /// In en, this message translates to:
  /// **'Override proxy'**
  String get overrideProxy;

  /// No description provided for @proxyOverride.
  ///
  /// In en, this message translates to:
  /// **'Proxy override'**
  String get proxyOverride;

  /// No description provided for @proxyHostPort.
  ///
  /// In en, this message translates to:
  /// **'host:port'**
  String get proxyHostPort;

  /// No description provided for @proxyHostPortHint.
  ///
  /// In en, this message translates to:
  /// **'127.0.0.1:8080'**
  String get proxyHostPortHint;

  /// No description provided for @proxyAutoDetect.
  ///
  /// In en, this message translates to:
  /// **'Leave empty to detect automatically'**
  String get proxyAutoDetect;

  /// No description provided for @copy.
  ///
  /// In en, this message translates to:
  /// **'Copy'**
  String get copy;

  /// No description provided for @copied.
  ///
  /// In en, this message translates to:
  /// **'Copied'**
  String get copied;

  /// No description provided for @noiseSuppression.
  ///
  /// In en, this message translates to:
  /// **'Noise suppression'**
  String get noiseSuppression;

  /// No description provided for @noiseOff.
  ///
  /// In en, this message translates to:
  /// **'Off'**
  String get noiseOff;

  /// No description provided for @noiseLight.
  ///
  /// In en, this message translates to:
  /// **'Light'**
  String get noiseLight;

  /// No description provided for @noiseStandard.
  ///
  /// In en, this message translates to:
  /// **'Standard'**
  String get noiseStandard;

  /// No description provided for @noiseHelmet.
  ///
  /// In en, this message translates to:
  /// **'Helmet / motorcycle'**
  String get noiseHelmet;

  /// No description provided for @noiseAuto.
  ///
  /// In en, this message translates to:
  /// **'Automatic'**
  String get noiseAuto;

  /// No description provided for @micMode.
  ///
  /// In en, this message translates to:
  /// **'Microphone mode'**
  String get micMode;

  /// No description provided for @micPushToTalk.
  ///
  /// In en, this message translates to:
  /// **'Push to talk'**
  String get micPushToTalk;

  /// No description provided for @micVoiceActivated.
  ///
  /// In en, this message translates to:
  /// **'Voice activated'**
  String get micVoiceActivated;

  /// No description provided for @micContinuous.
  ///
  /// In en, this message translates to:
  /// **'Open mic'**
  String get micContinuous;

  /// No description provided for @buttons.
  ///
  /// In en, this message translates to:
  /// **'Buttons'**
  String get buttons;

  /// No description provided for @addBinding.
  ///
  /// In en, this message translates to:
  /// **'Add a button…'**
  String get addBinding;

  /// No description provided for @removeBinding.
  ///
  /// In en, this message translates to:
  /// **'Remove binding'**
  String get removeBinding;

  /// No description provided for @action.
  ///
  /// In en, this message translates to:
  /// **'Action'**
  String get action;

  /// No description provided for @pressAButton.
  ///
  /// In en, this message translates to:
  /// **'Press the button you want to use'**
  String get pressAButton;

  /// No description provided for @waitingForButton.
  ///
  /// In en, this message translates to:
  /// **'Waiting…'**
  String get waitingForButton;

  /// No description provided for @buttonActionTalk.
  ///
  /// In en, this message translates to:
  /// **'Hold to talk'**
  String get buttonActionTalk;

  /// No description provided for @buttonActionToggleTalk.
  ///
  /// In en, this message translates to:
  /// **'Toggle transmit'**
  String get buttonActionToggleTalk;

  /// No description provided for @buttonActionToggleMute.
  ///
  /// In en, this message translates to:
  /// **'Toggle mute'**
  String get buttonActionToggleMute;

  /// No description provided for @buttonActionToggleDeafen.
  ///
  /// In en, this message translates to:
  /// **'Toggle deafen'**
  String get buttonActionToggleDeafen;

  /// No description provided for @floatingWindow.
  ///
  /// In en, this message translates to:
  /// **'Show floating call window'**
  String get floatingWindow;

  /// No description provided for @identityFingerprint.
  ///
  /// In en, this message translates to:
  /// **'Your certificate fingerprint'**
  String get identityFingerprint;

  /// No description provided for @reverb.
  ///
  /// In en, this message translates to:
  /// **'Room tone'**
  String get reverb;

  /// No description provided for @reverbBody.
  ///
  /// In en, this message translates to:
  /// **'Adds a short tail under incoming voices, so a talker who is cut off by voice activation does not stop mid-breath.'**
  String get reverbBody;

  /// Title of the setting that forces the cheaper speech-enhancement model
  ///
  /// In en, this message translates to:
  /// **'Light noise model'**
  String get simpleModel;

  /// Explains the trade behind the light noise model setting: cheaper to run and keeps the rest of the chain, at the cost of being harder on quiet voices and adding latency
  ///
  /// In en, this message translates to:
  /// **'Runs a smaller speech cleaner that costs a third as much to run. On a slow phone this keeps the rest of the noise chain working instead of it being switched off piece by piece. It is a little harsher on quiet speech, and adds 20 ms of delay.'**
  String get simpleModelBody;

  /// No description provided for @echoCancellation.
  ///
  /// In en, this message translates to:
  /// **'Echo cancellation'**
  String get echoCancellation;

  /// No description provided for @echoCancellationBody.
  ///
  /// In en, this message translates to:
  /// **'Removes what the speakers play back out of the microphone. Leave it on when using speakers; on a headset there is no echo to cancel and it can only take away.'**
  String get echoCancellationBody;

  /// No description provided for @noiseCancellation.
  ///
  /// In en, this message translates to:
  /// **'Noise cancellation'**
  String get noiseCancellation;

  /// No description provided for @noiseCancellationBody.
  ///
  /// In en, this message translates to:
  /// **'Filters wind, engine and road noise out of your microphone. Changes take effect next time the app starts.'**
  String get noiseCancellationBody;

  /// No description provided for @micModeBody.
  ///
  /// In en, this message translates to:
  /// **'Push-to-talk is the safest choice at speed: nothing you hit on the road opens the channel by accident.'**
  String get micModeBody;

  /// No description provided for @floatingCallWindow.
  ///
  /// In en, this message translates to:
  /// **'Floating call window'**
  String get floatingCallWindow;

  /// No description provided for @floatingCallWindowBody.
  ///
  /// In en, this message translates to:
  /// **'Keeps the call visible over whatever else is on screen, with the controls in reach without going back to the app.'**
  String get floatingCallWindowBody;

  /// No description provided for @buttonsBody.
  ///
  /// In en, this message translates to:
  /// **'Bind a handlebar Bluetooth remote, headset button or keyboard key. On Android these keep working with the app in the background while riding.'**
  String get buttonsBody;

  /// No description provided for @networkBody.
  ///
  /// In en, this message translates to:
  /// **'Downloads — the public server directory and profile files — go through the proxy configured here.'**
  String get networkBody;

  /// No description provided for @identity.
  ///
  /// In en, this message translates to:
  /// **'Identity'**
  String get identity;

  /// No description provided for @identityBody.
  ///
  /// In en, this message translates to:
  /// **'Mumble servers recognise you by a certificate this app generated. Give this fingerprint to a server admin to register your account.'**
  String get identityBody;

  /// No description provided for @noiseOffBody.
  ///
  /// In en, this message translates to:
  /// **'No suppression, only a gentle rumble filter.'**
  String get noiseOffBody;

  /// No description provided for @noiseLightBody.
  ///
  /// In en, this message translates to:
  /// **'Quiet indoor use; keeps the most natural sound.'**
  String get noiseLightBody;

  /// No description provided for @noiseStandardBody.
  ///
  /// In en, this message translates to:
  /// **'General purpose, for most environments.'**
  String get noiseStandardBody;

  /// No description provided for @noiseHelmetBody.
  ///
  /// In en, this message translates to:
  /// **'Steep wind-noise filter, full suppression and an assertive gate. Built for a microphone inside a helmet at speed.'**
  String get noiseHelmetBody;

  /// No description provided for @noiseAutoBody.
  ///
  /// In en, this message translates to:
  /// **'Listens to the background and picks one of the settings above. On a phone it also runs a small sound classifier: when it hears engine, wind or music it takes the helmet setting straight away and holds it for fifteen seconds after they stop. Going back down is slower — fifteen seconds of quiet to leave the helmet setting, and a minute more to reach the lightest. Useful when one ride covers a quiet car park and a motorway.'**
  String get noiseAutoBody;

  /// No description provided for @micAlwaysOn.
  ///
  /// In en, this message translates to:
  /// **'Always on'**
  String get micAlwaysOn;

  /// No description provided for @micPushToTalkBody.
  ///
  /// In en, this message translates to:
  /// **'Transmit only while holding the talk button.'**
  String get micPushToTalkBody;

  /// No description provided for @micVoiceActivatedBody.
  ///
  /// In en, this message translates to:
  /// **'Transmit automatically when you speak.'**
  String get micVoiceActivatedBody;

  /// No description provided for @micAlwaysOnBody.
  ///
  /// In en, this message translates to:
  /// **'Transmit constantly. Uses the most data.'**
  String get micAlwaysOnBody;

  /// No description provided for @platformRoutesAudio.
  ///
  /// In en, this message translates to:
  /// **'This platform routes audio automatically — connecting a headset switches to it.'**
  String get platformRoutesAudio;

  /// No description provided for @recheckDevicesBody.
  ///
  /// In en, this message translates to:
  /// **'After plugging in or pairing a headset'**
  String get recheckDevicesBody;

  /// No description provided for @testMicrophone.
  ///
  /// In en, this message translates to:
  /// **'Test microphone (hear yourself)'**
  String get testMicrophone;

  /// No description provided for @testMicrophoneBody.
  ///
  /// In en, this message translates to:
  /// **'Plays your processed voice back, exactly as the far end hears it. Use headphones: through speakers it becomes a feedback loop.'**
  String get testMicrophoneBody;

  /// No description provided for @testSpeakersBody.
  ///
  /// In en, this message translates to:
  /// **'Plays a short tone on the selected output'**
  String get testSpeakersBody;

  /// No description provided for @microphoneGain.
  ///
  /// In en, this message translates to:
  /// **'Microphone gain'**
  String get microphoneGain;

  /// No description provided for @levelsHint.
  ///
  /// In en, this message translates to:
  /// **'Aim for the meter to peak around three quarters while speaking normally.'**
  String get levelsHint;

  /// No description provided for @noButtonsBound.
  ///
  /// In en, this message translates to:
  /// **'No buttons bound yet.'**
  String get noButtonsBound;

  /// No description provided for @boundButton.
  ///
  /// In en, this message translates to:
  /// **'Bound {name}'**
  String boundButton(String name);

  /// No description provided for @learn.
  ///
  /// In en, this message translates to:
  /// **'Learn'**
  String get learn;

  /// No description provided for @pressButtonNow.
  ///
  /// In en, this message translates to:
  /// **'Press the button on your remote now…'**
  String get pressButtonNow;

  /// No description provided for @proxyOffDirect.
  ///
  /// In en, this message translates to:
  /// **'Off — connecting directly'**
  String get proxyOffDirect;

  /// No description provided for @certificateFingerprint.
  ///
  /// In en, this message translates to:
  /// **'Certificate fingerprint'**
  String get certificateFingerprint;

  /// No description provided for @inThisChannel.
  ///
  /// In en, this message translates to:
  /// **'In this channel ({count})'**
  String inThisChannel(int count);

  /// No description provided for @channelsHeading.
  ///
  /// In en, this message translates to:
  /// **'Channels ({count})'**
  String channelsHeading(int count);

  /// No description provided for @noChannelsYet.
  ///
  /// In en, this message translates to:
  /// **'No channels yet.'**
  String get noChannelsYet;

  /// No description provided for @nobodyElseHere.
  ///
  /// In en, this message translates to:
  /// **'Nobody else is in this channel.'**
  String get nobodyElseHere;

  /// No description provided for @joinAutomatically.
  ///
  /// In en, this message translates to:
  /// **'Join this channel automatically'**
  String get joinAutomatically;

  /// No description provided for @stopJoiningAutomatically.
  ///
  /// In en, this message translates to:
  /// **'Stop joining this channel automatically'**
  String get stopJoiningAutomatically;

  /// No description provided for @muteForMe.
  ///
  /// In en, this message translates to:
  /// **'Mute for me'**
  String get muteForMe;

  /// No description provided for @unmuteForMe.
  ///
  /// In en, this message translates to:
  /// **'Unmute for me'**
  String get unmuteForMe;

  /// No description provided for @muteOnServer.
  ///
  /// In en, this message translates to:
  /// **'Mute on server (for everyone)'**
  String get muteOnServer;

  /// No description provided for @unmuteOnServer.
  ///
  /// In en, this message translates to:
  /// **'Unmute on server'**
  String get unmuteOnServer;

  /// No description provided for @deafenOnServer.
  ///
  /// In en, this message translates to:
  /// **'Deafen on server'**
  String get deafenOnServer;

  /// No description provided for @undeafenOnServer.
  ///
  /// In en, this message translates to:
  /// **'Undeafen on server'**
  String get undeafenOnServer;

  /// No description provided for @kickFromServer.
  ///
  /// In en, this message translates to:
  /// **'Kick from server…'**
  String get kickFromServer;

  /// No description provided for @kickTitle.
  ///
  /// In en, this message translates to:
  /// **'Kick {name}?'**
  String kickTitle(String name);

  /// No description provided for @kickBody.
  ///
  /// In en, this message translates to:
  /// **'They will be disconnected from the server. This is not a ban — they can reconnect straight away.'**
  String get kickBody;

  /// No description provided for @kickReasonLabel.
  ///
  /// In en, this message translates to:
  /// **'Reason (optional)'**
  String get kickReasonLabel;

  /// No description provided for @kickReasonHint.
  ///
  /// In en, this message translates to:
  /// **'Shown to them as they are removed'**
  String get kickReasonHint;

  /// No description provided for @kick.
  ///
  /// In en, this message translates to:
  /// **'Kick'**
  String get kick;

  /// No description provided for @kickSent.
  ///
  /// In en, this message translates to:
  /// **'Kick sent. If nothing happens, you lack the Kick permission.'**
  String get kickSent;

  /// No description provided for @userStatusTalking.
  ///
  /// In en, this message translates to:
  /// **'talking'**
  String get userStatusTalking;

  /// No description provided for @userStatusSilent.
  ///
  /// In en, this message translates to:
  /// **'silent'**
  String get userStatusSilent;

  /// No description provided for @userStatusMuted.
  ///
  /// In en, this message translates to:
  /// **'muted'**
  String get userStatusMuted;

  /// No description provided for @userStatusDeafened.
  ///
  /// In en, this message translates to:
  /// **'deafened'**
  String get userStatusDeafened;

  /// No description provided for @userStatusMutedForYou.
  ///
  /// In en, this message translates to:
  /// **'muted for you'**
  String get userStatusMutedForYou;

  /// No description provided for @noServerSelected.
  ///
  /// In en, this message translates to:
  /// **'No server selected'**
  String get noServerSelected;

  /// No description provided for @noServerSelectedBody.
  ///
  /// In en, this message translates to:
  /// **'Add a server to see its channels and who is on it.'**
  String get noServerSelectedBody;

  /// No description provided for @connectToSeeChannels.
  ///
  /// In en, this message translates to:
  /// **'Connect to see the channel list and who is here.'**
  String get connectToSeeChannels;

  /// No description provided for @welcomeMessage.
  ///
  /// In en, this message translates to:
  /// **'Welcome message'**
  String get welcomeMessage;

  /// No description provided for @messages.
  ///
  /// In en, this message translates to:
  /// **'Messages'**
  String get messages;

  /// No description provided for @syncTitle.
  ///
  /// In en, this message translates to:
  /// **'Sync'**
  String get syncTitle;

  /// No description provided for @syncServers.
  ///
  /// In en, this message translates to:
  /// **'Sync servers and settings across devices'**
  String get syncServers;

  /// No description provided for @syncBodyICloud.
  ///
  /// In en, this message translates to:
  /// **'Your server list and your settings travel through iCloud to every device signed in to your Apple Account. Passwords go separately, through iCloud Keychain, which is end-to-end encrypted.'**
  String get syncBodyICloud;

  /// No description provided for @syncSignedOut.
  ///
  /// In en, this message translates to:
  /// **'Sign in to iCloud on this device to use this.'**
  String get syncSignedOut;

  /// No description provided for @syncNow.
  ///
  /// In en, this message translates to:
  /// **'Sync now'**
  String get syncNow;

  /// No description provided for @syncFailed.
  ///
  /// In en, this message translates to:
  /// **'Last sync failed: {error}'**
  String syncFailed(String error);

  /// No description provided for @transmissionIndicator.
  ///
  /// In en, this message translates to:
  /// **'Transmission indicator'**
  String get transmissionIndicator;

  /// No description provided for @diagnostics.
  ///
  /// In en, this message translates to:
  /// **'Diagnostics'**
  String get diagnostics;

  /// No description provided for @fingerprintCopied.
  ///
  /// In en, this message translates to:
  /// **'Fingerprint copied'**
  String get fingerprintCopied;

  /// No description provided for @evenOutLoudness.
  ///
  /// In en, this message translates to:
  /// **'Even out speaker loudness'**
  String get evenOutLoudness;

  /// No description provided for @evenOutLoudnessBody.
  ///
  /// In en, this message translates to:
  /// **'Brings everyone to a similar level. Adapts on what it hears, so if a hiss rises between sentences, turn this off to check.'**
  String get evenOutLoudnessBody;

  /// No description provided for @qrCodeTitle.
  ///
  /// In en, this message translates to:
  /// **'QR code'**
  String get qrCodeTitle;

  /// No description provided for @shareQrCode.
  ///
  /// In en, this message translates to:
  /// **'Share QR code'**
  String get shareQrCode;

  /// No description provided for @qrCarriesPassword.
  ///
  /// In en, this message translates to:
  /// **'This code contains the password. Anyone who can see it — including over your shoulder, or in a photograph — can connect as you.'**
  String get qrCarriesPassword;

  /// No description provided for @shareQrImage.
  ///
  /// In en, this message translates to:
  /// **'Share the code'**
  String get shareQrImage;

  /// No description provided for @copyMumbleUrl.
  ///
  /// In en, this message translates to:
  /// **'Copy mumble:// URL'**
  String get copyMumbleUrl;

  /// No description provided for @linkCopied.
  ///
  /// In en, this message translates to:
  /// **'Link copied'**
  String get linkCopied;

  /// No description provided for @qrCouldNotRender.
  ///
  /// In en, this message translates to:
  /// **'The code could not be drawn.'**
  String get qrCouldNotRender;

  /// Subject line when sharing an invite.
  ///
  /// In en, this message translates to:
  /// **'Join me on {name}'**
  String joinMeOn(String name);

  /// No description provided for @scanQrCode.
  ///
  /// In en, this message translates to:
  /// **'Scan a QR code'**
  String get scanQrCode;

  /// No description provided for @importQrImage.
  ///
  /// In en, this message translates to:
  /// **'Import a QR code image'**
  String get importQrImage;

  /// No description provided for @qrNoCodeFound.
  ///
  /// In en, this message translates to:
  /// **'No QR code was found in that image.'**
  String get qrNoCodeFound;

  /// No description provided for @qrNotAnInvite.
  ///
  /// In en, this message translates to:
  /// **'That code is not a MumbleWay invitation.'**
  String get qrNotAnInvite;

  /// No description provided for @qrCameraFailed.
  ///
  /// In en, this message translates to:
  /// **'MumbleWay could not start the camera. Another app may be using it, or this device may not support the preview MumbleWay asks for. You can import a picture of the code instead.'**
  String get qrCameraFailed;

  /// No description provided for @qrCameraDenied.
  ///
  /// In en, this message translates to:
  /// **'MumbleWay needs the camera to scan a code. Grant it in system settings and try again.'**
  String get qrCameraDenied;

  /// No description provided for @qrPointAtCode.
  ///
  /// In en, this message translates to:
  /// **'Point the camera at the code'**
  String get qrPointAtCode;

  /// No description provided for @jitterBuffer.
  ///
  /// In en, this message translates to:
  /// **'Incoming audio buffer'**
  String get jitterBuffer;

  /// No description provided for @jitterBufferBody.
  ///
  /// In en, this message translates to:
  /// **'How much of what others say is held back before it is played. More buffer rides out a patchy signal without gaps; less means you hear them sooner. MumbleWay adds to it by itself when a link starts losing packets, and comes back down to this. Raise it if the playback gaps counter in Diagnostics keeps climbing.'**
  String get jitterBufferBody;

  /// A duration in milliseconds, shown beside a slider.
  ///
  /// In en, this message translates to:
  /// **'{ms} ms'**
  String milliseconds(int ms);

  /// No description provided for @notAvailableHere.
  ///
  /// In en, this message translates to:
  /// **'Not available on this platform.'**
  String get notAvailableHere;

  /// No description provided for @pasteLinkOrProfile.
  ///
  /// In en, this message translates to:
  /// **'Paste a link or profile'**
  String get pasteLinkOrProfile;

  /// No description provided for @downloadProfileFile.
  ///
  /// In en, this message translates to:
  /// **'Download a profile file'**
  String get downloadProfileFile;

  /// No description provided for @downloadAndAdd.
  ///
  /// In en, this message translates to:
  /// **'Download and add'**
  String get downloadAndAdd;

  /// No description provided for @chooseUsername.
  ///
  /// In en, this message translates to:
  /// **'Choose a username'**
  String get chooseUsername;

  /// No description provided for @chooseUsernameHelp.
  ///
  /// In en, this message translates to:
  /// **'How others on the server will see you'**
  String get chooseUsernameHelp;

  /// No description provided for @directConnection.
  ///
  /// In en, this message translates to:
  /// **'Direct connection'**
  String get directConnection;

  /// No description provided for @tunnelledOverTcp.
  ///
  /// In en, this message translates to:
  /// **'Tunnelled over TCP because UDP is blocked'**
  String get tunnelledOverTcp;

  /// No description provided for @floatingNotAvailable.
  ///
  /// In en, this message translates to:
  /// **'Floating windows are not available here.'**
  String get floatingNotAvailable;

  /// No description provided for @floatingCouldNotShow.
  ///
  /// In en, this message translates to:
  /// **'Could not show the floating window.'**
  String get floatingCouldNotShow;

  /// No description provided for @allowOverlayFirst.
  ///
  /// In en, this message translates to:
  /// **'Allow \"display over other apps\" first.'**
  String get allowOverlayFirst;

  /// No description provided for @microphonePermissionNeeded.
  ///
  /// In en, this message translates to:
  /// **'MumbleWay needs permission to use the microphone. Allow it in Settings, then reopen the app.'**
  String get microphonePermissionNeeded;

  /// No description provided for @noAudioInput.
  ///
  /// In en, this message translates to:
  /// **'This device is not offering any audio input right now. If a headset is connected, try reconnecting it.'**
  String get noAudioInput;

  /// No description provided for @serverNoLongerInList.
  ///
  /// In en, this message translates to:
  /// **'That server is no longer in your list.'**
  String get serverNoLongerInList;

  /// No description provided for @serversAlreadyAdded.
  ///
  /// In en, this message translates to:
  /// **'Those servers are already in your list.'**
  String get serversAlreadyAdded;

  /// No description provided for @noServersToExport.
  ///
  /// In en, this message translates to:
  /// **'There are no servers to export.'**
  String get noServersToExport;

  /// No description provided for @serverProfilesFileType.
  ///
  /// In en, this message translates to:
  /// **'Server profiles'**
  String get serverProfilesFileType;

  /// No description provided for @diagIncomingAudio.
  ///
  /// In en, this message translates to:
  /// **'Incoming audio'**
  String get diagIncomingAudio;

  /// No description provided for @diagInvented.
  ///
  /// In en, this message translates to:
  /// **'Invented to cover gaps'**
  String get diagInvented;

  /// No description provided for @diagGapsConcealed.
  ///
  /// In en, this message translates to:
  /// **'Gaps concealed'**
  String get diagGapsConcealed;

  /// No description provided for @diagSpeakersTracked.
  ///
  /// In en, this message translates to:
  /// **'Speakers tracked'**
  String get diagSpeakersTracked;

  /// No description provided for @diagMicrophoneDropped.
  ///
  /// In en, this message translates to:
  /// **'Microphone dropped'**
  String get diagMicrophoneDropped;

  /// No description provided for @diagInputPeak.
  ///
  /// In en, this message translates to:
  /// **'Microphone peak'**
  String get diagInputPeak;

  /// No description provided for @diagInputClipped.
  ///
  /// In en, this message translates to:
  /// **'Microphone clipped'**
  String get diagInputClipped;

  /// No description provided for @diagMicrophoneLevel.
  ///
  /// In en, this message translates to:
  /// **'After suppression'**
  String get diagMicrophoneLevel;

  /// No description provided for @diagReconnectAttempts.
  ///
  /// In en, this message translates to:
  /// **'Reconnect attempts'**
  String get diagReconnectAttempts;

  /// No description provided for @diagReset.
  ///
  /// In en, this message translates to:
  /// **'Reset'**
  String get diagReset;

  /// No description provided for @diagClose.
  ///
  /// In en, this message translates to:
  /// **'Close'**
  String get diagClose;

  /// No description provided for @diagDecoded.
  ///
  /// In en, this message translates to:
  /// **'Decoded'**
  String get diagDecoded;

  /// No description provided for @diagJitterBuffer.
  ///
  /// In en, this message translates to:
  /// **'Jitter buffer'**
  String get diagJitterBuffer;

  /// No description provided for @diagThisDevice.
  ///
  /// In en, this message translates to:
  /// **'This device'**
  String get diagThisDevice;

  /// No description provided for @diagPlaybackGaps.
  ///
  /// In en, this message translates to:
  /// **'Playback gaps'**
  String get diagPlaybackGaps;

  /// No description provided for @diagNoiseFloor.
  ///
  /// In en, this message translates to:
  /// **'Noise floor'**
  String get diagNoiseFloor;

  /// No description provided for @diagOpensAt.
  ///
  /// In en, this message translates to:
  /// **'Opens at'**
  String get diagOpensAt;

  /// No description provided for @diagNetwork.
  ///
  /// In en, this message translates to:
  /// **'Network'**
  String get diagNetwork;

  /// No description provided for @diagVoicePackets.
  ///
  /// In en, this message translates to:
  /// **'Voice packets'**
  String get diagVoicePackets;

  /// No description provided for @diagMemory.
  ///
  /// In en, this message translates to:
  /// **'Memory'**
  String get diagMemory;

  /// No description provided for @diagVoicePath.
  ///
  /// In en, this message translates to:
  /// **'Voice path'**
  String get diagVoicePath;

  /// No description provided for @diagUdpDirect.
  ///
  /// In en, this message translates to:
  /// **'UDP direct'**
  String get diagUdpDirect;

  /// No description provided for @diagTcpTunnelled.
  ///
  /// In en, this message translates to:
  /// **'TCP tunnelled'**
  String get diagTcpTunnelled;

  /// No description provided for @diagPing.
  ///
  /// In en, this message translates to:
  /// **'Ping'**
  String get diagPing;

  /// No description provided for @diagInChannel.
  ///
  /// In en, this message translates to:
  /// **'In channel'**
  String get diagInChannel;

  /// No description provided for @diagParticipants.
  ///
  /// In en, this message translates to:
  /// **'Participants'**
  String get diagParticipants;

  /// No description provided for @diagRecording.
  ///
  /// In en, this message translates to:
  /// **'Record for diagnosis'**
  String get diagRecording;

  /// No description provided for @diagRecordingBody.
  ///
  /// In en, this message translates to:
  /// **'Saves your microphone to this device.'**
  String get diagRecordingBody;

  /// No description provided for @diagRecordingShared.
  ///
  /// In en, this message translates to:
  /// **'Shared {count} files in {archives, plural, =1{one archive} other{{archives} archives}}.'**
  String diagRecordingShared(int count, int archives);

  /// Shown in place of the spectrum analyser once the performance ladder has switched it off to keep the speech enhancer running
  ///
  /// In en, this message translates to:
  /// **'The analyser is switched off. This device could not process audio fast enough, and drawing it was costing more than the voice could spare.'**
  String get diagAnalyserGivenUp;

  /// Warning shown when the performance ladder has reduced the enhancer but not yet switched any stage off, so nothing is crossed out in the dots above
  ///
  /// In en, this message translates to:
  /// **'This device could not process audio fast enough, so the noise chain is doing less work than it would otherwise. Your voice still goes out, but it will sound worse than it would on a faster phone.'**
  String get diagChainReduced;

  /// Tooltip on the toolbar diagnostics button once it has turned into a warning
  ///
  /// In en, this message translates to:
  /// **'Parts of the noise chain are switched off'**
  String get diagChainDegradedShort;

  /// Tooltip on the toolbar diagnostics button while the startup performance probe is measuring the capture chain, and the button shows a spinner instead of an icon
  ///
  /// In en, this message translates to:
  /// **'Checking what this device can run'**
  String get diagProbing;

  /// Warning shown in diagnostics when the performance ladder has disabled one or more capture chain stages, telling the rider quality is affected and suggesting a faster device
  ///
  /// In en, this message translates to:
  /// **'This device could not process audio fast enough, so parts of the noise chain have been switched off — they are crossed out above. Your voice still goes out, but it will sound worse than it would on a faster phone. A more powerful device would run the whole chain.'**
  String get diagChainDegraded;

  /// Label for the row naming which effort rung the speech enhancer is running at
  ///
  /// In en, this message translates to:
  /// **'Enhancer'**
  String get diagEnhancerEffort;

  /// The speech enhancer is doing all the work it can
  ///
  /// In en, this message translates to:
  /// **'Full'**
  String get diagEnhancerRungFull;

  /// The speech enhancer stepped down one rung: deepest filtering only where most needed
  ///
  /// In en, this message translates to:
  /// **'Reduced'**
  String get diagEnhancerRungReduced;

  /// The speech enhancer is running its light stage only; the deepest filtering never runs
  ///
  /// In en, this message translates to:
  /// **'Light'**
  String get diagEnhancerRungLight;

  /// The speech enhancer has been bypassed entirely for this session
  ///
  /// In en, this message translates to:
  /// **'Off'**
  String get diagEnhancerRungOff;

  /// Shown where the top background classes go, while the model is running but has not produced a result yet
  ///
  /// In en, this message translates to:
  /// **'Listening to the background…'**
  String get diagClassifierListening;

  /// Shown in diagnostics when the speech enhancer has dropped to its reduced setting because the device missed the block deadline
  ///
  /// In en, this message translates to:
  /// **'This device could not keep up, so the enhancer stepped down. It still runs, with the deepest filtering only on the frames that most need it.'**
  String get diagEnhancerReduced;

  /// Shown in diagnostics when the speech enhancer has dropped to its lightest setting
  ///
  /// In en, this message translates to:
  /// **'This device could not keep up, so the enhancer is running its light stage only. Speech still comes through; the deepest filtering does not run.'**
  String get diagEnhancerErbOnly;

  /// Shown in diagnostics when the speech enhancer has been bypassed entirely
  ///
  /// In en, this message translates to:
  /// **'This device could not keep up even at the lightest setting, so the enhancer is switched off for this session.'**
  String get diagEnhancerBypassed;

  /// No description provided for @diagPreviewTitle.
  ///
  /// In en, this message translates to:
  /// **'Listen back'**
  String get diagPreviewTitle;

  /// No description provided for @diagPreviewBody.
  ///
  /// In en, this message translates to:
  /// **'It is a recording of your own microphone. Hear what is in it before you send it anywhere.'**
  String get diagPreviewBody;

  /// No description provided for @diagPreviewPlay.
  ///
  /// In en, this message translates to:
  /// **'Play'**
  String get diagPreviewPlay;

  /// No description provided for @diagPreviewPause.
  ///
  /// In en, this message translates to:
  /// **'Pause'**
  String get diagPreviewPause;

  /// Tooltip on the delete button in the listen-back sheet
  ///
  /// In en, this message translates to:
  /// **'Delete this recording'**
  String get diagPreviewDelete;

  /// Tooltip on the control that skips everything the gate rejected, when it is off
  ///
  /// In en, this message translates to:
  /// **'Play only what was transmitted'**
  String get diagPreviewSentOnly;

  /// Tooltip on the same control when it is on, saying what pressing it does next
  ///
  /// In en, this message translates to:
  /// **'Play the whole recording'**
  String get diagPreviewSentOnlyOff;

  /// Title of the group breaking one capture block down by stage
  ///
  /// In en, this message translates to:
  /// **'Where a block\'s 10 ms goes'**
  String get diagBlockCost;

  /// No description provided for @diagStageInput.
  ///
  /// In en, this message translates to:
  /// **'Input and taps'**
  String get diagStageInput;

  /// No description provided for @diagStageEnhancer.
  ///
  /// In en, this message translates to:
  /// **'Enhancer'**
  String get diagStageEnhancer;

  /// No description provided for @diagStageSuppression.
  ///
  /// In en, this message translates to:
  /// **'Suppression'**
  String get diagStageSuppression;

  /// No description provided for @diagStageFeedback.
  ///
  /// In en, this message translates to:
  /// **'Feedback'**
  String get diagStageFeedback;

  /// No description provided for @diagStageDehiss.
  ///
  /// In en, this message translates to:
  /// **'De-hiss'**
  String get diagStageDehiss;

  /// No description provided for @diagStageTransmit.
  ///
  /// In en, this message translates to:
  /// **'To the server'**
  String get diagStageTransmit;

  /// No description provided for @diagStageEncode.
  ///
  /// In en, this message translates to:
  /// **'Encode'**
  String get diagStageEncode;

  /// The part of a block no stage was timing: scheduling, mostly. A large value means the worker is being interrupted rather than running slowly
  ///
  /// In en, this message translates to:
  /// **'Not in any stage'**
  String get diagBlockUnattributed;

  /// No description provided for @diagBlockTotal.
  ///
  /// In en, this message translates to:
  /// **'Whole block, mean / worst'**
  String get diagBlockTotal;

  /// How much captured audio was queued when a block started. A backlog that grows means the chain cannot keep up
  ///
  /// In en, this message translates to:
  /// **'Waiting to be processed, mean / worst'**
  String get diagBlockBacklog;

  /// Tooltip on the same control when it is unavailable, either because the recording has no decision log or because nothing in it went out
  ///
  /// In en, this message translates to:
  /// **'Nothing in this recording was transmitted'**
  String get diagPreviewSentOnlyNone;

  /// Tooltip on the control that runs playback through the capture chain, when it is off
  ///
  /// In en, this message translates to:
  /// **'Play through the noise chain, to hear what the others hear'**
  String get diagPreviewChain;

  /// Tooltip on the same control when it is on, saying what pressing it does next
  ///
  /// In en, this message translates to:
  /// **'Play the microphone as it was recorded'**
  String get diagPreviewChainOff;

  /// Shown under a waveform with no transmitted parts, when the decision log says the microphone was muted
  ///
  /// In en, this message translates to:
  /// **'None of this went out: the microphone was muted.'**
  String get diagPreviewNoneMuted;

  /// Shown under a waveform with no transmitted parts, when the mode was push-to-talk throughout
  ///
  /// In en, this message translates to:
  /// **'None of this went out: push-to-talk was set, and the button was not pressed.'**
  String get diagPreviewNonePushToTalk;

  /// Shown under a waveform with no transmitted parts when the log does not say why, either because it predates the mode and muted columns or because the gate never opened
  ///
  /// In en, this message translates to:
  /// **'None of this went out. The recording is still worth sending — the log says why.'**
  String get diagPreviewNoneUnexplained;

  /// Tooltip on the share button in the listen-back sheet
  ///
  /// In en, this message translates to:
  /// **'Share this recording'**
  String get diagPreviewShare;

  /// Title of the confirmation for deleting one recording
  ///
  /// In en, this message translates to:
  /// **'Delete this recording?'**
  String get diagPreviewDeleteTitle;

  /// Body of the confirmation for deleting one recording
  ///
  /// In en, this message translates to:
  /// **'{name} and its decision log go from this device. A ride cannot be recorded again.'**
  String diagPreviewDeleteBody(String name);

  /// Shown when a recording could not be deleted because a handle was still open
  ///
  /// In en, this message translates to:
  /// **'That recording is still in use and was not deleted. Try again in a moment.'**
  String get diagPreviewDeleteFailed;

  /// No description provided for @diagRecordingListen.
  ///
  /// In en, this message translates to:
  /// **'Listen to recordings'**
  String get diagRecordingListen;

  /// No description provided for @diagRecordingDiscardTitle.
  ///
  /// In en, this message translates to:
  /// **'Delete recordings?'**
  String get diagRecordingDiscardTitle;

  /// No description provided for @diagRecordingDiscardBody.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{One file goes from this device, and a ride cannot be recorded again.} other{{count} files go from this device, and a ride cannot be recorded again.}}'**
  String diagRecordingDiscardBody(int count);

  /// No description provided for @diagRecordingActive.
  ///
  /// In en, this message translates to:
  /// **'Recording'**
  String get diagRecordingActive;

  /// No description provided for @diagRecordingStopped.
  ///
  /// In en, this message translates to:
  /// **'Recorded {count} files'**
  String diagRecordingStopped(int count);

  /// No description provided for @diagRecordingDropped.
  ///
  /// In en, this message translates to:
  /// **'{count} blocks lost — storage could not keep up'**
  String diagRecordingDropped(int count);

  /// No description provided for @diagRecordingShare.
  ///
  /// In en, this message translates to:
  /// **'Share recordings'**
  String get diagRecordingShare;

  /// No description provided for @diagRecordingDiscard.
  ///
  /// In en, this message translates to:
  /// **'Delete recordings'**
  String get diagRecordingDiscard;

  /// No description provided for @diagRecordingNone.
  ///
  /// In en, this message translates to:
  /// **'Nothing recorded yet'**
  String get diagRecordingNone;

  /// No description provided for @diagRecordingSize.
  ///
  /// In en, this message translates to:
  /// **'{megabytes} MB on this device'**
  String diagRecordingSize(String megabytes);

  /// No description provided for @diagRecordingFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not start recording: {reason}'**
  String diagRecordingFailed(String reason);

  /// No description provided for @diagRecordingShareFailed.
  ///
  /// In en, this message translates to:
  /// **'Could not share the recordings: {reason}'**
  String diagRecordingShareFailed(String reason);

  /// No description provided for @levelsHelp.
  ///
  /// In en, this message translates to:
  /// **'Aim for the meter to peak around three quarters while speaking normally. Too much gain lifts the engine noise with your voice.'**
  String get levelsHelp;

  /// No description provided for @floatingAndroidBody.
  ///
  /// In en, this message translates to:
  /// **'Talk, mute, deafen and hang up over other apps. Needs the \"display over other apps\" permission.'**
  String get floatingAndroidBody;

  /// No description provided for @floatingIosBody.
  ///
  /// In en, this message translates to:
  /// **'Picture in Picture, appearing when you leave the app. The system allows three buttons: play/pause talks, skip back mutes, skip forward hangs up (twice to confirm).'**
  String get floatingIosBody;

  /// No description provided for @actionPushToTalkHold.
  ///
  /// In en, this message translates to:
  /// **'Push to talk (hold)'**
  String get actionPushToTalkHold;

  /// No description provided for @actionPushToTalkToggle.
  ///
  /// In en, this message translates to:
  /// **'Push to talk (toggle)'**
  String get actionPushToTalkToggle;

  /// No description provided for @actionToggleMute.
  ///
  /// In en, this message translates to:
  /// **'Mute / unmute'**
  String get actionToggleMute;

  /// No description provided for @actionToggleDeafen.
  ///
  /// In en, this message translates to:
  /// **'Deafen / undeafen'**
  String get actionToggleDeafen;

  /// No description provided for @buttonsIosNote.
  ///
  /// In en, this message translates to:
  /// **'A Bluetooth remote reports its media buttons as a tap, never as a hold, so push-to-talk (hold) cannot work from one. Use the toggle action instead. While a media button is bound, the remote controls MumbleWay rather than your music app.'**
  String get buttonsIosNote;

  /// No description provided for @remoteListening.
  ///
  /// In en, this message translates to:
  /// **'Listening for a remote'**
  String get remoteListening;

  /// No description provided for @remoteNothingYet.
  ///
  /// In en, this message translates to:
  /// **'no button received yet'**
  String get remoteNothingYet;

  /// No description provided for @remoteLastButton.
  ///
  /// In en, this message translates to:
  /// **'last button: {name}'**
  String remoteLastButton(String name);

  /// No description provided for @pipOnAir.
  ///
  /// In en, this message translates to:
  /// **'ON AIR'**
  String get pipOnAir;

  /// No description provided for @pipTalking.
  ///
  /// In en, this message translates to:
  /// **'Talking'**
  String get pipTalking;

  /// No description provided for @pipDeafened.
  ///
  /// In en, this message translates to:
  /// **'Deafened'**
  String get pipDeafened;

  /// No description provided for @pipMuted.
  ///
  /// In en, this message translates to:
  /// **'Muted'**
  String get pipMuted;

  /// No description provided for @pipListening.
  ///
  /// In en, this message translates to:
  /// **'Listening, but\nnot transmitting'**
  String get pipListening;

  /// No description provided for @pipBadgeMuted.
  ///
  /// In en, this message translates to:
  /// **'MUTED'**
  String get pipBadgeMuted;

  /// No description provided for @pipBadgeDeafened.
  ///
  /// In en, this message translates to:
  /// **'DEAFENED'**
  String get pipBadgeDeafened;

  /// No description provided for @pipNoise.
  ///
  /// In en, this message translates to:
  /// **'noise'**
  String get pipNoise;

  /// No description provided for @pipOpen.
  ///
  /// In en, this message translates to:
  /// **'open'**
  String get pipOpen;

  /// No description provided for @pipTalk.
  ///
  /// In en, this message translates to:
  /// **'talk'**
  String get pipTalk;

  /// Accessibility label for the close button on the Android floating call window. It hides the window until the rider opens the app and leaves it again; it does not end the call or turn the feature off
  ///
  /// In en, this message translates to:
  /// **'Hide this window'**
  String get pipClose;

  /// No description provided for @pipHandsFreeVoice.
  ///
  /// In en, this message translates to:
  /// **'hands-free · voice activated'**
  String get pipHandsFreeVoice;

  /// No description provided for @pipHandsFreeAlways.
  ///
  /// In en, this message translates to:
  /// **'hands-free · always on'**
  String get pipHandsFreeAlways;

  /// No description provided for @pipSpeaking.
  ///
  /// In en, this message translates to:
  /// **'SPEAKING'**
  String get pipSpeaking;

  /// No description provided for @pipNobodySpeaks.
  ///
  /// In en, this message translates to:
  /// **'Nobody speaks'**
  String get pipNobodySpeaks;

  /// No description provided for @pipNotConnected.
  ///
  /// In en, this message translates to:
  /// **'Not connected'**
  String get pipNotConnected;

  /// No description provided for @pipNoConnection.
  ///
  /// In en, this message translates to:
  /// **'No connection'**
  String get pipNoConnection;

  /// No description provided for @pipConnected.
  ///
  /// In en, this message translates to:
  /// **'Connected'**
  String get pipConnected;

  /// No description provided for @pipConnectedCount.
  ///
  /// In en, this message translates to:
  /// **'{count} connected'**
  String pipConnectedCount(int count);

  /// No description provided for @pipReconnecting.
  ///
  /// In en, this message translates to:
  /// **'Reconnecting…'**
  String get pipReconnecting;

  /// No description provided for @pipUpAndReconnecting.
  ///
  /// In en, this message translates to:
  /// **'{up} up · {count} reconnecting'**
  String pipUpAndReconnecting(int up, int count);

  /// No description provided for @pipMoreSpeakers.
  ///
  /// In en, this message translates to:
  /// **'+{count} more'**
  String pipMoreSpeakers(int count);

  /// No description provided for @pipOthersOnline.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 other person online} other{{count} other people online}}'**
  String pipOthersOnline(int count);

  /// No description provided for @pipNobodyElse.
  ///
  /// In en, this message translates to:
  /// **'Nobody else is here now'**
  String get pipNobodyElse;

  /// No description provided for @feedbackGuard.
  ///
  /// In en, this message translates to:
  /// **'Feedback suppression'**
  String get feedbackGuard;

  /// No description provided for @feedbackGuardBody.
  ///
  /// In en, this message translates to:
  /// **'For when the speaker is heard by the microphone. Echo cancellation removes what it can predict; these handle what is left, and they work in quite different ways.'**
  String get feedbackGuardBody;

  /// No description provided for @feedbackOff.
  ///
  /// In en, this message translates to:
  /// **'No feedback suppression'**
  String get feedbackOff;

  /// No description provided for @feedbackOffBody.
  ///
  /// In en, this message translates to:
  /// **'Echo cancellation alone. Start here, and change it only if you hear yourself coming back or a howl builds up.'**
  String get feedbackOffBody;

  /// No description provided for @feedbackDuck.
  ///
  /// In en, this message translates to:
  /// **'Turn the microphone down while others talk'**
  String get feedbackDuck;

  /// No description provided for @feedbackDuckBody.
  ///
  /// In en, this message translates to:
  /// **'What intercoms have always done, and the most effective with a speaker close to the microphone in a helmet. The cost is that talking over somebody becomes harder.'**
  String get feedbackDuckBody;

  /// No description provided for @feedbackHowl.
  ///
  /// In en, this message translates to:
  /// **'Cut only when a howl builds'**
  String get feedbackHowl;

  /// No description provided for @feedbackHowlBody.
  ///
  /// In en, this message translates to:
  /// **'Leaves ordinary conversation completely alone and cuts hard the moment a tone starts climbing. Does nothing about mild bleed.'**
  String get feedbackHowlBody;

  /// No description provided for @feedbackResidual.
  ///
  /// In en, this message translates to:
  /// **'Suppress whatever echo cancellation missed'**
  String get feedbackResidual;

  /// No description provided for @feedbackResidualBody.
  ///
  /// In en, this message translates to:
  /// **'Attenuates in proportion to how much of the sound looks like the far end rather than you. The gentlest on a real conversation, and the weakest against a genuine howl.'**
  String get feedbackResidualBody;

  /// No description provided for @dehiss.
  ///
  /// In en, this message translates to:
  /// **'Hiss removal'**
  String get dehiss;

  /// No description provided for @dehissBody.
  ///
  /// In en, this message translates to:
  /// **'For the steady hiss a microphone adds under everything. Separate from noise suppression, which handles the road and the wind: those are loud and change with speed, while hiss is quiet, high and unvarying.'**
  String get dehissBody;

  /// No description provided for @dehissOff.
  ///
  /// In en, this message translates to:
  /// **'No hiss removal'**
  String get dehissOff;

  /// No description provided for @dehissOffBody.
  ///
  /// In en, this message translates to:
  /// **'Leaves the sound alone. Start here — both of the others discard something, and a link that already sounds fine is not worth changing.'**
  String get dehissOffBody;

  /// No description provided for @dehissExpander.
  ///
  /// In en, this message translates to:
  /// **'Turn quiet passages down further'**
  String get dehissExpander;

  /// No description provided for @dehissExpanderBody.
  ///
  /// In en, this message translates to:
  /// **'Attenuates in proportion to how far below the noise floor the sound sits, so speech is untouched and the gaps between words go quiet. Cannot make a voice sound processed; can make the background breathe.'**
  String get dehissExpanderBody;

  /// No description provided for @dehissSpectral.
  ///
  /// In en, this message translates to:
  /// **'Learn the hiss and subtract it'**
  String get dehissSpectral;

  /// No description provided for @dehissSpectralBody.
  ///
  /// In en, this message translates to:
  /// **'Measures the noise while nobody is talking and removes it frequency by frequency, so hiss goes from under speech as well as from the gaps. The strongest option, and the one that can leave a faint flicker behind it.'**
  String get dehissSpectralBody;

  /// No description provided for @serverBusyChange.
  ///
  /// In en, this message translates to:
  /// **'Disconnect from this server before changing or removing it.'**
  String get serverBusyChange;

  /// No description provided for @disconnectFirst.
  ///
  /// In en, this message translates to:
  /// **'Disconnect first'**
  String get disconnectFirst;

  /// No description provided for @diagLog.
  ///
  /// In en, this message translates to:
  /// **'Engine log'**
  String get diagLog;

  /// No description provided for @diagLogProblems.
  ///
  /// In en, this message translates to:
  /// **'Problems only'**
  String get diagLogProblems;

  /// No description provided for @diagLogAll.
  ///
  /// In en, this message translates to:
  /// **'Show all'**
  String get diagLogAll;

  /// No description provided for @diagLogCopy.
  ///
  /// In en, this message translates to:
  /// **'Copy the whole log'**
  String get diagLogCopy;

  /// No description provided for @diagLogCopied.
  ///
  /// In en, this message translates to:
  /// **'Log copied to the clipboard.'**
  String get diagLogCopied;

  /// No description provided for @diagLogClear.
  ///
  /// In en, this message translates to:
  /// **'Clear the log'**
  String get diagLogClear;

  /// No description provided for @diagLogEmpty.
  ///
  /// In en, this message translates to:
  /// **'Nothing logged yet.'**
  String get diagLogEmpty;

  /// No description provided for @diagLogNoProblems.
  ///
  /// In en, this message translates to:
  /// **'No warnings or errors.'**
  String get diagLogNoProblems;

  /// No description provided for @diagAutoProfile.
  ///
  /// In en, this message translates to:
  /// **'Auto is using'**
  String get diagAutoProfile;

  /// No description provided for @diagStageBackground.
  ///
  /// In en, this message translates to:
  /// **'Background'**
  String get diagStageBackground;

  /// No description provided for @diagClassifierOnCpu.
  ///
  /// In en, this message translates to:
  /// **'No accelerator here, so background detection runs on the processor — {ms} ms per check, once every two seconds.'**
  String diagClassifierOnCpu(String ms);

  /// No description provided for @diagClassifierUnavailable.
  ///
  /// In en, this message translates to:
  /// **'Background detection runs on phones only, so the helmet profile is chosen from levels here.'**
  String get diagClassifierUnavailable;

  /// No description provided for @diagSpectrum.
  ///
  /// In en, this message translates to:
  /// **'Voice chain'**
  String get diagSpectrum;

  /// No description provided for @diagSpectrumWaiting.
  ///
  /// In en, this message translates to:
  /// **'Waiting for audio'**
  String get diagSpectrumWaiting;

  /// No description provided for @diagSpectrumStalled.
  ///
  /// In en, this message translates to:
  /// **'The audio engine has stopped'**
  String get diagSpectrumStalled;

  /// No description provided for @diagTraceRaw.
  ///
  /// In en, this message translates to:
  /// **'Microphone'**
  String get diagTraceRaw;

  /// No description provided for @diagTracePreGate.
  ///
  /// In en, this message translates to:
  /// **'After suppression'**
  String get diagTracePreGate;

  /// No description provided for @diagTraceSentLive.
  ///
  /// In en, this message translates to:
  /// **'Sending'**
  String get diagTraceSentLive;

  /// No description provided for @diagTraceSentIdle.
  ///
  /// In en, this message translates to:
  /// **'Not sending'**
  String get diagTraceSentIdle;

  /// No description provided for @diagStageEcho.
  ///
  /// In en, this message translates to:
  /// **'Echo'**
  String get diagStageEcho;

  /// No description provided for @diagStageSuppressor.
  ///
  /// In en, this message translates to:
  /// **'Suppressor'**
  String get diagStageSuppressor;

  /// No description provided for @diagStageVoice.
  ///
  /// In en, this message translates to:
  /// **'Voice detected'**
  String get diagStageVoice;

  /// No description provided for @diagStageGate.
  ///
  /// In en, this message translates to:
  /// **'Gate'**
  String get diagStageGate;

  /// No description provided for @diagStageLevel.
  ///
  /// In en, this message translates to:
  /// **'Levelling'**
  String get diagStageLevel;

  /// No description provided for @diagStageHiss.
  ///
  /// In en, this message translates to:
  /// **'Hiss'**
  String get diagStageHiss;

  /// Menu item opening the app’s own website in a browser.
  ///
  /// In en, this message translates to:
  /// **'Website'**
  String get website;

  /// Tooltip on the wordmark, which opens the website.
  ///
  /// In en, this message translates to:
  /// **'Open the MumbleWay website'**
  String get openWebsite;

  /// Tooltip on the help button in the settings title bar.
  ///
  /// In en, this message translates to:
  /// **'Help for this screen'**
  String get helpForThisScreen;

  /// Shown when the platform refuses to open a web address.
  ///
  /// In en, this message translates to:
  /// **'Could not open the link. No browser answered.'**
  String get couldNotOpenLink;

  /// Snackbar shown when the server refuses an action.
  ///
  /// In en, this message translates to:
  /// **'The server said no: {reason}'**
  String serverRefused(String reason);

  /// No description provided for @denyText.
  ///
  /// In en, this message translates to:
  /// **'The server would not deliver that message.'**
  String get denyText;

  /// No description provided for @denyPermission.
  ///
  /// In en, this message translates to:
  /// **'The server refused: you do not have permission for that.'**
  String get denyPermission;

  /// No description provided for @denySuperUser.
  ///
  /// In en, this message translates to:
  /// **'That account cannot be changed from a client.'**
  String get denySuperUser;

  /// No description provided for @denyChannelName.
  ///
  /// In en, this message translates to:
  /// **'The server would not accept that channel name.'**
  String get denyChannelName;

  /// No description provided for @denyTextTooLong.
  ///
  /// In en, this message translates to:
  /// **'That message is longer than the server allows.'**
  String get denyTextTooLong;

  /// No description provided for @denyTemporaryChannel.
  ///
  /// In en, this message translates to:
  /// **'That cannot be done in a temporary channel.'**
  String get denyTemporaryChannel;

  /// No description provided for @denyMissingCertificate.
  ///
  /// In en, this message translates to:
  /// **'The server needs a certificate for that.'**
  String get denyMissingCertificate;

  /// No description provided for @denyUserName.
  ///
  /// In en, this message translates to:
  /// **'The server would not accept that name.'**
  String get denyUserName;

  /// No description provided for @denyChannelFull.
  ///
  /// In en, this message translates to:
  /// **'That channel is full.'**
  String get denyChannelFull;

  /// No description provided for @denyNestingLimit.
  ///
  /// In en, this message translates to:
  /// **'Channels cannot be nested any deeper on this server.'**
  String get denyNestingLimit;

  /// No description provided for @denyChannelCountLimit.
  ///
  /// In en, this message translates to:
  /// **'The server has as many channels as it allows.'**
  String get denyChannelCountLimit;

  /// No description provided for @denyListenerLimit.
  ///
  /// In en, this message translates to:
  /// **'The server has reached its listener limit.'**
  String get denyListenerLimit;
}

class _LDelegate extends LocalizationsDelegate<L> {
  const _LDelegate();

  @override
  Future<L> load(Locale locale) {
    return SynchronousFuture<L>(lookupL(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en', 'ru'].contains(locale.languageCode);

  @override
  bool shouldReload(_LDelegate old) => false;
}

L lookupL(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return LEn();
    case 'ru':
      return LRu();
  }

  throw FlutterError(
    'L.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
