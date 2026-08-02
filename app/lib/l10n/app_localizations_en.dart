// ignore: unused_import
import 'package:intl/intl.dart' as intl;
import 'app_localizations.dart';

// ignore_for_file: type=lint

/// The translations for English (`en`).
class LEn extends L {
  LEn([String locale = 'en']) : super(locale);

  @override
  String get appTitle => 'MumbleWay';

  @override
  String get cancel => 'Cancel';

  @override
  String get save => 'Save';

  @override
  String get add => 'Add';

  @override
  String get remove => 'Remove';

  @override
  String get settings => 'Settings';

  @override
  String get language => 'Language';

  @override
  String get deafen => 'Deafen';

  @override
  String get undeafen => 'Undeafen';

  @override
  String get muteMicrophone => 'Mute microphone';

  @override
  String get unmuteMicrophone => 'Unmute microphone';

  @override
  String get exportServers => 'Export servers…';

  @override
  String get importFromFile => 'Import from file…';

  @override
  String get noServersTitle => 'No servers yet';

  @override
  String get noServersBody =>
      'Add a Mumble server to start talking. You can stay connected to two at once.';

  @override
  String get addServer => 'Add server';

  @override
  String get addAnotherServer => 'Add another server';

  @override
  String maxServersNote(int count) {
    return 'Up to $count servers can be connected at once; the rest stay saved.';
  }

  @override
  String get notConnectedAny => 'Not connected to any server';

  @override
  String get talkingOnOne => 'Talking on 1 server';

  @override
  String talkingOnMany(int count) {
    return 'Talking on $count servers simultaneously';
  }

  @override
  String get audioFailedTitle => 'Audio could not start';

  @override
  String get audioFailedBody =>
      'MumbleWay needs a microphone. Check that one is connected and that permission is granted, then restart the app.';

  @override
  String get statusConnected => 'Connected';

  @override
  String get statusConnecting => 'Connecting';

  @override
  String get statusAuthenticating => 'Authenticating';

  @override
  String get statusReconnecting => 'Reconnecting';

  @override
  String get statusError => 'Error';

  @override
  String get statusDisconnected => 'Disconnected';

  @override
  String get statusNotConnected => 'Not connected';

  @override
  String get pttHoldToTalk => 'HOLD TO TALK';

  @override
  String get pttTransmitting => 'TRANSMITTING';

  @override
  String get pttMicrophoneMuted => 'MICROPHONE MUTED';

  @override
  String get pttVoiceActivated => 'VOICE ACTIVATED';

  @override
  String get pttOpenMic => 'OPEN MIC';

  @override
  String get probeChecking => 'Checking…';

  @override
  String get probeNotResponding => 'Not responding';

  @override
  String get connect => 'Connect';

  @override
  String get disconnect => 'Disconnect';

  @override
  String get joining => 'joining…';

  @override
  String get shareInviteLink => 'Share invite link';

  @override
  String get shareProfileFile => 'Share profile file';

  @override
  String get duplicate => 'Duplicate';

  @override
  String get removeServerTitle => 'Remove server?';

  @override
  String removeServerBody(String name) {
    return '$name will be removed from your list.';
  }

  @override
  String get includePasswordTitle => 'Include the password?';

  @override
  String get includePasswordBody =>
      'Anyone who receives this can join without being asked for a password. It stays valid for as long as the password does, wherever the message ends up.';

  @override
  String get withoutPassword => 'Without password';

  @override
  String get includeIt => 'Include it';

  @override
  String get certChangedTitle => 'Server certificate changed';

  @override
  String get certChangedBody =>
      'This can mean the server was reinstalled — or that someone is impersonating it. Only continue if you expected this.';

  @override
  String get trustNewCertificate => 'Trust the new certificate';

  @override
  String reconnectingIn(int seconds, int attempt) {
    return 'Connection lost. Retrying in ${seconds}s (attempt $attempt).';
  }

  @override
  String get connectionLost => 'Connection lost.';

  @override
  String retryingInSeconds(int seconds, int attempt) {
    return 'Retrying in ${seconds}s (attempt $attempt).';
  }

  @override
  String retryingNow(int attempt) {
    return 'Retrying now (attempt $attempt)…';
  }

  @override
  String switchToLanguage(String name) {
    return 'Tap to switch to $name';
  }

  @override
  String get more => 'More';

  @override
  String get edit => 'Edit';

  @override
  String get editServer => 'Edit server';

  @override
  String get saveChanges => 'Save changes';

  @override
  String get savingChanges => 'Saving…';

  @override
  String get displayName => 'Display name';

  @override
  String get displayNameHint => 'Sunday ride';

  @override
  String get displayNameMissing => 'Give it a name';

  @override
  String get serverAddress => 'Server address';

  @override
  String get serverAddressHint => 'mumble.example.com';

  @override
  String get serverAddressMissing => 'Enter an address';

  @override
  String get port => 'Port';

  @override
  String get portOutOfRange => 'Port 1-65535';

  @override
  String get username => 'Username';

  @override
  String get usernameMissing => 'Enter a username';

  @override
  String get passwordOptional => 'Password (optional)';

  @override
  String get passwordHelp => 'Only if the server requires one';

  @override
  String get addingServer => 'Adding…';

  @override
  String get quickerWays => 'Quicker ways to add a server';

  @override
  String get browsePublic => 'Browse public';

  @override
  String get importLabel => 'Import';

  @override
  String get publicServers => 'Public servers';

  @override
  String get search => 'Search';

  @override
  String get reload => 'Reload';

  @override
  String get addToMyServers => 'Add to my servers';

  @override
  String get noServersMatchSearch => 'No servers match that search.';

  @override
  String get importServers => 'Import servers';

  @override
  String get addFromText => 'Add from text';

  @override
  String get profileFileFormat => 'Profile file format';

  @override
  String get serversAdded => 'Servers added';

  @override
  String get audioDevices => 'Audio devices';

  @override
  String get levels => 'Levels';

  @override
  String get network => 'Network';

  @override
  String get microphone => 'Microphone';

  @override
  String get speakers => 'Speakers';

  @override
  String get systemDefault => 'System default';

  @override
  String get detectedAutomatically => 'Detected automatically';

  @override
  String get recheckDevices => 'Re-check devices';

  @override
  String get testSpeakers => 'Test speakers';

  @override
  String get play => 'Play';

  @override
  String get stop => 'Stop';

  @override
  String get speakerVolume => 'Speaker volume';

  @override
  String get inputGain => 'Input gain';

  @override
  String get hearMyself => 'Hear myself';

  @override
  String get hearMyselfHelp =>
      'Plays your processed voice back. Use headphones — on speakers it will feed back.';

  @override
  String get useSystemProxy => 'Use the system proxy';

  @override
  String get overrideProxy => 'Override proxy';

  @override
  String get proxyOverride => 'Proxy override';

  @override
  String get proxyHostPort => 'host:port';

  @override
  String get proxyHostPortHint => '127.0.0.1:8080';

  @override
  String get proxyAutoDetect => 'Leave empty to detect automatically';

  @override
  String get copy => 'Copy';

  @override
  String get copied => 'Copied';

  @override
  String get noiseSuppression => 'Noise suppression';

  @override
  String get noiseOff => 'Off';

  @override
  String get noiseLight => 'Light';

  @override
  String get noiseStandard => 'Standard';

  @override
  String get noiseHelmet => 'Helmet / motorcycle';

  @override
  String get micMode => 'Microphone mode';

  @override
  String get micPushToTalk => 'Push to talk';

  @override
  String get micVoiceActivated => 'Voice activated';

  @override
  String get micContinuous => 'Open mic';

  @override
  String get buttons => 'Buttons';

  @override
  String get addBinding => 'Add a button…';

  @override
  String get removeBinding => 'Remove binding';

  @override
  String get action => 'Action';

  @override
  String get pressAButton => 'Press the button you want to use';

  @override
  String get waitingForButton => 'Waiting…';

  @override
  String get buttonActionTalk => 'Hold to talk';

  @override
  String get buttonActionToggleTalk => 'Toggle transmit';

  @override
  String get buttonActionToggleMute => 'Toggle mute';

  @override
  String get buttonActionToggleDeafen => 'Toggle deafen';

  @override
  String get floatingWindow => 'Show floating call window';

  @override
  String get identityFingerprint => 'Your certificate fingerprint';

  @override
  String get reverb => 'Room tone';

  @override
  String get reverbBody =>
      'Adds a short tail under incoming voices, so a talker who is cut off by voice activation does not stop mid-breath.';

  @override
  String get echoCancellation => 'Echo cancellation';

  @override
  String get echoCancellationBody =>
      'Removes what the speakers play back out of the microphone. Leave it on when using speakers; on a headset there is no echo to cancel and it can only take away.';

  @override
  String get noiseCancellation => 'Noise cancellation';

  @override
  String get noiseCancellationBody =>
      'Filters wind, engine and road noise out of your microphone. Changes take effect next time the app starts.';

  @override
  String get micModeBody =>
      'Push-to-talk is the safest choice at speed: nothing you hit on the road opens the channel by accident.';

  @override
  String get floatingTalkButton => 'Floating talk button';

  @override
  String get floatingTalkButtonBody =>
      'Puts a small draggable push-to-talk button over whatever else is on screen.';

  @override
  String get buttonsBody =>
      'Bind a handlebar Bluetooth remote, headset button or keyboard key. On Android these keep working with the app in the background while riding.';

  @override
  String get networkBody =>
      'Downloads — the public server directory and profile files — go through the proxy configured here.';

  @override
  String get identity => 'Identity';

  @override
  String get identityBody =>
      'Mumble servers recognise you by a certificate this app generated. Give this fingerprint to a server admin to register your account.';

  @override
  String get noiseOffBody => 'No suppression, only a gentle rumble filter.';

  @override
  String get noiseLightBody =>
      'Quiet indoor use; keeps the most natural sound.';

  @override
  String get noiseStandardBody => 'General purpose, for most environments.';

  @override
  String get noiseHelmetBody =>
      'Steep wind-noise filter, full suppression and an assertive gate. Built for a microphone inside a helmet at speed.';

  @override
  String get micAlwaysOn => 'Always on';

  @override
  String get micPushToTalkBody =>
      'Transmit only while holding the talk button.';

  @override
  String get micVoiceActivatedBody => 'Transmit automatically when you speak.';

  @override
  String get micAlwaysOnBody => 'Transmit constantly. Uses the most data.';

  @override
  String get platformRoutesAudio =>
      'This platform routes audio automatically — connecting a headset switches to it.';

  @override
  String get recheckDevicesBody => 'After plugging in or pairing a headset';

  @override
  String get testMicrophone => 'Test microphone (hear yourself)';

  @override
  String get testMicrophoneBody =>
      'Plays your processed voice back, exactly as the far end hears it. Use headphones: through speakers it becomes a feedback loop.';

  @override
  String get testSpeakersBody => 'Plays a short tone on the selected output';

  @override
  String get microphoneGain => 'Microphone gain';

  @override
  String get levelsHint =>
      'Aim for the meter to peak around three quarters while speaking normally.';

  @override
  String get noButtonsBound => 'No buttons bound yet.';

  @override
  String boundButton(String name) {
    return 'Bound $name';
  }

  @override
  String get learn => 'Learn';

  @override
  String get pressButtonNow => 'Press the button on your remote now…';

  @override
  String get proxyOffDirect => 'Off — connecting directly';

  @override
  String get certificateFingerprint => 'Certificate fingerprint';

  @override
  String inThisChannel(int count) {
    return 'In this channel ($count)';
  }

  @override
  String channelsHeading(int count) {
    return 'Channels ($count)';
  }

  @override
  String get noChannelsYet => 'No channels yet.';

  @override
  String get nobodyElseHere => 'Nobody else is in this channel.';

  @override
  String get joinAutomatically => 'Join this channel automatically';

  @override
  String get stopJoiningAutomatically =>
      'Stop joining this channel automatically';

  @override
  String get muteForMe => 'Mute for me';

  @override
  String get unmuteForMe => 'Unmute for me';

  @override
  String get muteOnServer => 'Mute on server (for everyone)';

  @override
  String get unmuteOnServer => 'Unmute on server';

  @override
  String get deafenOnServer => 'Deafen on server';

  @override
  String get undeafenOnServer => 'Undeafen on server';

  @override
  String get kickFromServer => 'Kick from server…';

  @override
  String kickTitle(String name) {
    return 'Kick $name?';
  }

  @override
  String get kickBody =>
      'They will be disconnected from the server. This is not a ban — they can reconnect straight away.';

  @override
  String get kickReasonLabel => 'Reason (optional)';

  @override
  String get kickReasonHint => 'Shown to them as they are removed';

  @override
  String get kick => 'Kick';

  @override
  String get kickSent =>
      'Kick sent. If nothing happens, you lack the Kick permission.';

  @override
  String get userStatusTalking => 'talking';

  @override
  String get userStatusSilent => 'silent';

  @override
  String get userStatusMuted => 'muted';

  @override
  String get userStatusDeafened => 'deafened';

  @override
  String get userStatusMutedForYou => 'muted for you';

  @override
  String get noServerSelected => 'No server selected';

  @override
  String get noServerSelectedBody =>
      'Add a server to see its channels and who is on it.';

  @override
  String get connectToSeeChannels =>
      'Connect to see the channel list and who is here.';

  @override
  String get welcomeMessage => 'Welcome message';

  @override
  String get messages => 'Messages';
}
