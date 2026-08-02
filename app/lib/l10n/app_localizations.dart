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
