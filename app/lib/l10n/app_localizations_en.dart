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
  String get delete => 'Delete';

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
  String allSlotsInUse(int count) {
    return 'Already talking on $count servers. Leave one first.';
  }

  @override
  String get micIdleWithTalkButton =>
      'The talk button and microphone meter appear here once you connect to a server.';

  @override
  String get micIdleMeterOnly =>
      'The microphone meter appears here once you connect to a server.';

  @override
  String get micIdleWhy =>
      'The microphone stays closed until then, so nothing is recorded and your headset keeps its sound quality for other apps.';

  @override
  String get micUnavailable =>
      'The microphone could not be opened. Another app may be using it.';

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
  String get noiseAuto => 'Automatic';

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
  String get simpleModel => 'Light noise model';

  @override
  String get simpleModelBody =>
      'Runs a smaller speech cleaner that costs a third as much to run. On a slow phone this keeps the rest of the noise chain working instead of it being switched off piece by piece. It is a little harsher on quiet speech, and adds 20 ms of delay.';

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
  String get floatingCallWindow => 'Floating call window';

  @override
  String get floatingCallWindowBody =>
      'Keeps the call visible over whatever else is on screen, with the controls in reach without going back to the app.';

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
  String get noiseAutoBody =>
      'Listens to the background and picks one of the settings above. On a phone it also runs a small sound classifier: when it hears engine, wind or music it takes the helmet setting straight away and holds it for fifteen seconds after they stop. Going back down is slower — fifteen seconds of quiet to leave the helmet setting, and a minute more to reach the lightest. Useful when one ride covers a quiet car park and a motorway.';

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

  @override
  String get syncTitle => 'Sync';

  @override
  String get syncServers => 'Sync servers and settings across devices';

  @override
  String get syncBodyICloud =>
      'Your server list and your settings travel through iCloud to every device signed in to your Apple Account. Passwords go separately, through iCloud Keychain, which is end-to-end encrypted.';

  @override
  String get syncSignedOut => 'Sign in to iCloud on this device to use this.';

  @override
  String get syncNow => 'Sync now';

  @override
  String syncFailed(String error) {
    return 'Last sync failed: $error';
  }

  @override
  String get transmissionIndicator => 'Transmission indicator';

  @override
  String get diagnostics => 'Diagnostics';

  @override
  String get fingerprintCopied => 'Fingerprint copied';

  @override
  String get evenOutLoudness => 'Even out speaker loudness';

  @override
  String get evenOutLoudnessBody =>
      'Brings everyone to a similar level. Adapts on what it hears, so if a hiss rises between sentences, turn this off to check.';

  @override
  String get qrCodeTitle => 'QR code';

  @override
  String get shareQrCode => 'Share QR code';

  @override
  String get qrCarriesPassword =>
      'This code contains the password. Anyone who can see it — including over your shoulder, or in a photograph — can connect as you.';

  @override
  String get shareQrImage => 'Share the code';

  @override
  String get copyMumbleUrl => 'Copy mumble:// URL';

  @override
  String get linkCopied => 'Link copied';

  @override
  String get qrCouldNotRender => 'The code could not be drawn.';

  @override
  String joinMeOn(String name) {
    return 'Join me on $name';
  }

  @override
  String get scanQrCode => 'Scan a QR code';

  @override
  String get importQrImage => 'Import a QR code image';

  @override
  String get qrNoCodeFound => 'No QR code was found in that image.';

  @override
  String get qrNotAnInvite => 'That code is not a MumbleWay invitation.';

  @override
  String get qrCameraFailed =>
      'MumbleWay could not start the camera. Another app may be using it, or this device may not support the preview MumbleWay asks for. You can import a picture of the code instead.';

  @override
  String get qrCameraDenied =>
      'MumbleWay needs the camera to scan a code. Grant it in system settings and try again.';

  @override
  String get qrPointAtCode => 'Point the camera at the code';

  @override
  String get jitterBuffer => 'Incoming audio buffer';

  @override
  String get jitterBufferBody =>
      'How much of what others say is held back before it is played. More buffer rides out a patchy signal without gaps; less means you hear them sooner. MumbleWay adds to it by itself when a link starts losing packets, and comes back down to this. Raise it if the playback gaps counter in Diagnostics keeps climbing.';

  @override
  String milliseconds(int ms) {
    return '$ms ms';
  }

  @override
  String get notAvailableHere => 'Not available on this platform.';

  @override
  String get pasteLinkOrProfile => 'Paste a link or profile';

  @override
  String get downloadProfileFile => 'Download a profile file';

  @override
  String get downloadAndAdd => 'Download and add';

  @override
  String get chooseUsername => 'Choose a username';

  @override
  String get chooseUsernameHelp => 'How others on the server will see you';

  @override
  String get directConnection => 'Direct connection';

  @override
  String get tunnelledOverTcp => 'Tunnelled over TCP because UDP is blocked';

  @override
  String get floatingNotAvailable => 'Floating windows are not available here.';

  @override
  String get floatingCouldNotShow => 'Could not show the floating window.';

  @override
  String get allowOverlayFirst => 'Allow \"display over other apps\" first.';

  @override
  String get microphonePermissionNeeded =>
      'MumbleWay needs permission to use the microphone. Allow it in Settings, then reopen the app.';

  @override
  String get noAudioInput =>
      'This device is not offering any audio input right now. If a headset is connected, try reconnecting it.';

  @override
  String get serverNoLongerInList => 'That server is no longer in your list.';

  @override
  String get serversAlreadyAdded => 'Those servers are already in your list.';

  @override
  String get noServersToExport => 'There are no servers to export.';

  @override
  String get serverProfilesFileType => 'Server profiles';

  @override
  String get diagIncomingAudio => 'Incoming audio';

  @override
  String get diagInvented => 'Invented to cover gaps';

  @override
  String get diagGapsConcealed => 'Gaps concealed';

  @override
  String get diagSpeakersTracked => 'Speakers tracked';

  @override
  String get diagMicrophoneDropped => 'Microphone dropped';

  @override
  String get diagInputPeak => 'Microphone peak';

  @override
  String get diagInputClipped => 'Microphone clipped';

  @override
  String get diagMicrophoneLevel => 'After suppression';

  @override
  String get diagReconnectAttempts => 'Reconnect attempts';

  @override
  String get diagReset => 'Reset';

  @override
  String get diagClose => 'Close';

  @override
  String get diagDecoded => 'Decoded';

  @override
  String get diagJitterBuffer => 'Jitter buffer';

  @override
  String get diagThisDevice => 'This device';

  @override
  String get diagPlaybackGaps => 'Playback gaps';

  @override
  String get diagNoiseFloor => 'Noise floor';

  @override
  String get diagOpensAt => 'Opens at';

  @override
  String get diagNetwork => 'Network';

  @override
  String get diagVoicePackets => 'Voice packets';

  @override
  String get diagMemory => 'Memory';

  @override
  String get diagVoicePath => 'Voice path';

  @override
  String get diagUdpDirect => 'UDP direct';

  @override
  String get diagTcpTunnelled => 'TCP tunnelled';

  @override
  String get diagPing => 'Ping';

  @override
  String get diagInChannel => 'In channel';

  @override
  String get diagParticipants => 'Participants';

  @override
  String get diagRecording => 'Record for diagnosis';

  @override
  String get diagRecordingBody => 'Saves your microphone to this device.';

  @override
  String diagRecordingShared(int count, int archives) {
    String _temp0 = intl.Intl.pluralLogic(
      archives,
      locale: localeName,
      other: '$archives archives',
      one: 'one archive',
    );
    return 'Shared $count files in $_temp0.';
  }

  @override
  String get diagAnalyserGivenUp =>
      'The analyser is switched off. This device could not process audio fast enough, and drawing it was costing more than the voice could spare.';

  @override
  String get diagChainReduced =>
      'This device could not process audio fast enough, so the noise chain is doing less work than it would otherwise. Your voice still goes out, but it will sound worse than it would on a faster phone.';

  @override
  String get diagChainDegradedShort =>
      'Parts of the noise chain are switched off';

  @override
  String get diagProbing => 'Checking what this device can run';

  @override
  String get diagChainDegraded =>
      'This device could not process audio fast enough, so parts of the noise chain have been switched off — they are crossed out above. Your voice still goes out, but it will sound worse than it would on a faster phone. A more powerful device would run the whole chain.';

  @override
  String get diagEnhancerEffort => 'Enhancer';

  @override
  String get diagPerCoreUnavailable =>
      'Per-core figures are not available on this device: the system will not report them to an app.';

  @override
  String get diagEnhancerModel => 'Model';

  @override
  String get diagEnhancerModelFull => 'Low latency';

  @override
  String get diagEnhancerModelSimple => 'Light';

  @override
  String get diagEnhancerRungFull => 'Full';

  @override
  String get diagEnhancerRungReduced => 'Reduced';

  @override
  String get diagEnhancerRungLight => 'Light';

  @override
  String get diagEnhancerRungOff => 'Off';

  @override
  String get diagClassifierListening => 'Listening to the background…';

  @override
  String get diagEnhancerReduced =>
      'This device could not keep up, so the enhancer stepped down. It still runs, with the deepest filtering only on the frames that most need it.';

  @override
  String get diagEnhancerErbOnly =>
      'This device could not keep up, so the enhancer is running its light stage only. Speech still comes through; the deepest filtering does not run.';

  @override
  String get diagEnhancerBypassed =>
      'This device could not keep up even at the lightest setting, so the enhancer is switched off for this session.';

  @override
  String get diagPreviewTitle => 'Listen back';

  @override
  String get diagPreviewBody =>
      'It is a recording of your own microphone. Hear what is in it before you send it anywhere.';

  @override
  String get diagPreviewPlay => 'Play';

  @override
  String get diagPreviewPause => 'Pause';

  @override
  String get diagPreviewDelete => 'Delete this recording';

  @override
  String get diagPreviewSentOnly => 'Play only what was transmitted';

  @override
  String get diagPreviewSentOnlyOff => 'Play the whole recording';

  @override
  String get diagBlockCost => 'Where a block\'s 10 ms goes';

  @override
  String get diagStageInput => 'Input and taps';

  @override
  String get diagStageEnhancer => 'Enhancer';

  @override
  String get diagStageSuppression => 'Suppression';

  @override
  String get diagStageFeedback => 'Feedback';

  @override
  String get diagStageDehiss => 'De-hiss';

  @override
  String get diagStageTransmit => 'To the server';

  @override
  String get diagStageEncode => 'Encode';

  @override
  String get diagBlockUnattributed => 'Not in any stage';

  @override
  String get diagBlockTotal => 'Whole block, mean / worst';

  @override
  String get diagBlockBacklog => 'Waiting to be processed, mean / worst';

  @override
  String get diagPreviewSentOnlyNone =>
      'Nothing in this recording was transmitted';

  @override
  String get diagPreviewChain =>
      'Play through the noise chain, to hear what the others hear';

  @override
  String get diagPreviewChainOff => 'Play the microphone as it was recorded';

  @override
  String get diagPreviewNoneMuted =>
      'None of this went out: the microphone was muted.';

  @override
  String get diagPreviewNonePushToTalk =>
      'None of this went out: push-to-talk was set, and the button was not pressed.';

  @override
  String get diagPreviewNoneUnexplained =>
      'None of this went out. The recording is still worth sending — the log says why.';

  @override
  String get diagPreviewShare => 'Share this recording';

  @override
  String get diagPreviewDeleteTitle => 'Delete this recording?';

  @override
  String diagPreviewDeleteBody(String name) {
    return '$name and its decision log go from this device. A ride cannot be recorded again.';
  }

  @override
  String get diagPreviewDeleteFailed =>
      'That recording is still in use and was not deleted. Try again in a moment.';

  @override
  String get diagRecordingListen => 'Listen to recordings';

  @override
  String get diagRecordingDiscardTitle => 'Delete recordings?';

  @override
  String diagRecordingDiscardBody(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other:
          '$count files go from this device, and a ride cannot be recorded again.',
      one:
          'One file goes from this device, and a ride cannot be recorded again.',
    );
    return '$_temp0';
  }

  @override
  String get diagRecordingActive => 'Recording';

  @override
  String diagRecordingStopped(int count) {
    return 'Recorded $count files';
  }

  @override
  String diagRecordingDropped(int count) {
    return '$count blocks lost — storage could not keep up';
  }

  @override
  String get diagRecordingShare => 'Share recordings';

  @override
  String get diagRecordingDiscard => 'Delete recordings';

  @override
  String get diagRecordingNone => 'Nothing recorded yet';

  @override
  String diagRecordingSize(String megabytes) {
    return '$megabytes MB on this device';
  }

  @override
  String diagRecordingFailed(String reason) {
    return 'Could not start recording: $reason';
  }

  @override
  String diagRecordingShareFailed(String reason) {
    return 'Could not share the recordings: $reason';
  }

  @override
  String get levelsHelp =>
      'Aim for the meter to peak around three quarters while speaking normally. Too much gain lifts the engine noise with your voice.';

  @override
  String get floatingAndroidBody =>
      'Talk, mute, deafen and hang up over other apps. Needs the \"display over other apps\" permission.';

  @override
  String get floatingIosBody =>
      'Picture in Picture, appearing when you leave the app. The system allows three buttons: play/pause talks, skip back mutes, skip forward hangs up (twice to confirm).';

  @override
  String get actionPushToTalkHold => 'Push to talk (hold)';

  @override
  String get actionPushToTalkToggle => 'Push to talk (toggle)';

  @override
  String get actionToggleMute => 'Mute / unmute';

  @override
  String get actionToggleDeafen => 'Deafen / undeafen';

  @override
  String get buttonsIosNote =>
      'A Bluetooth remote reports its media buttons as a tap, never as a hold, so push-to-talk (hold) cannot work from one. Use the toggle action instead. While a media button is bound, the remote controls MumbleWay rather than your music app.';

  @override
  String get remoteListening => 'Listening for a remote';

  @override
  String get remoteNothingYet => 'no button received yet';

  @override
  String remoteLastButton(String name) {
    return 'last button: $name';
  }

  @override
  String get pipOnAir => 'ON AIR';

  @override
  String get pipTalking => 'Talking';

  @override
  String get pipDeafened => 'Deafened';

  @override
  String get pipMuted => 'Muted';

  @override
  String get pipListening => 'Listening, but\nnot transmitting';

  @override
  String get pipBadgeMuted => 'MUTED';

  @override
  String get pipBadgeDeafened => 'DEAFENED';

  @override
  String get pipNoise => 'noise';

  @override
  String get pipOpen => 'open';

  @override
  String get pipTalk => 'talk';

  @override
  String get pipClose => 'Hide this window';

  @override
  String get pipHandsFreeVoice => 'hands-free · voice activated';

  @override
  String get pipHandsFreeAlways => 'hands-free · always on';

  @override
  String get pipSpeaking => 'SPEAKING';

  @override
  String get pipNobodySpeaks => 'Nobody speaks';

  @override
  String get pipNotConnected => 'Not connected';

  @override
  String get pipNoConnection => 'No connection';

  @override
  String get pipConnected => 'Connected';

  @override
  String pipConnectedCount(int count) {
    return '$count connected';
  }

  @override
  String get pipReconnecting => 'Reconnecting…';

  @override
  String pipUpAndReconnecting(int up, int count) {
    return '$up up · $count reconnecting';
  }

  @override
  String pipMoreSpeakers(int count) {
    return '+$count more';
  }

  @override
  String pipOthersOnline(int count) {
    String _temp0 = intl.Intl.pluralLogic(
      count,
      locale: localeName,
      other: '$count other people online',
      one: '1 other person online',
    );
    return '$_temp0';
  }

  @override
  String get pipNobodyElse => 'Nobody else is here now';

  @override
  String get feedbackGuard => 'Feedback suppression';

  @override
  String get feedbackGuardBody =>
      'For when the speaker is heard by the microphone. Echo cancellation removes what it can predict; these handle what is left, and they work in quite different ways.';

  @override
  String get feedbackOff => 'No feedback suppression';

  @override
  String get feedbackOffBody =>
      'Echo cancellation alone. Start here, and change it only if you hear yourself coming back or a howl builds up.';

  @override
  String get feedbackDuck => 'Turn the microphone down while others talk';

  @override
  String get feedbackDuckBody =>
      'What intercoms have always done, and the most effective with a speaker close to the microphone in a helmet. The cost is that talking over somebody becomes harder.';

  @override
  String get feedbackHowl => 'Cut only when a howl builds';

  @override
  String get feedbackHowlBody =>
      'Leaves ordinary conversation completely alone and cuts hard the moment a tone starts climbing. Does nothing about mild bleed.';

  @override
  String get feedbackResidual => 'Suppress whatever echo cancellation missed';

  @override
  String get feedbackResidualBody =>
      'Attenuates in proportion to how much of the sound looks like the far end rather than you. The gentlest on a real conversation, and the weakest against a genuine howl.';

  @override
  String get dehiss => 'Hiss removal';

  @override
  String get dehissBody =>
      'For the steady hiss a microphone adds under everything. Separate from noise suppression, which handles the road and the wind: those are loud and change with speed, while hiss is quiet, high and unvarying.';

  @override
  String get dehissOff => 'No hiss removal';

  @override
  String get dehissOffBody =>
      'Leaves the sound alone. Start here — both of the others discard something, and a link that already sounds fine is not worth changing.';

  @override
  String get dehissExpander => 'Turn quiet passages down further';

  @override
  String get dehissExpanderBody =>
      'Attenuates in proportion to how far below the noise floor the sound sits, so speech is untouched and the gaps between words go quiet. Cannot make a voice sound processed; can make the background breathe.';

  @override
  String get dehissSpectral => 'Learn the hiss and subtract it';

  @override
  String get dehissSpectralBody =>
      'Measures the noise while nobody is talking and removes it frequency by frequency, so hiss goes from under speech as well as from the gaps. The strongest option, and the one that can leave a faint flicker behind it.';

  @override
  String get serverBusyChange =>
      'Disconnect from this server before changing or removing it.';

  @override
  String get disconnectFirst => 'Disconnect first';

  @override
  String get diagLog => 'Engine log';

  @override
  String get diagLogProblems => 'Problems only';

  @override
  String get diagLogAll => 'Show all';

  @override
  String get diagLogCopy => 'Copy the whole log';

  @override
  String get diagLogCopied => 'Log copied to the clipboard.';

  @override
  String get diagLogClear => 'Clear the log';

  @override
  String get diagLogEmpty => 'Nothing logged yet.';

  @override
  String get diagLogNoProblems => 'No warnings or errors.';

  @override
  String get diagAutoProfile => 'Auto is using';

  @override
  String get diagChosenProfile => 'Profile';

  @override
  String get diagProfilePinned => '(pinned)';

  @override
  String get diagStageBackground => 'Background';

  @override
  String diagClassifierOnCpu(String ms) {
    return 'No accelerator here, so background detection runs on the processor — $ms ms per check, once every two seconds.';
  }

  @override
  String get diagClassifierUnavailable =>
      'Background detection runs on phones only, so the helmet profile is chosen from levels here.';

  @override
  String get diagSpectrum => 'Voice chain';

  @override
  String get diagSpectrumWaiting => 'Waiting for audio';

  @override
  String get diagSpectrumStalled => 'The audio engine has stopped';

  @override
  String get diagTraceRaw => 'Microphone';

  @override
  String get diagTracePreGate => 'After suppression';

  @override
  String get diagTraceSentLive => 'Sending';

  @override
  String get diagTraceSentIdle => 'Not sending';

  @override
  String get diagStageEcho => 'Echo';

  @override
  String get diagStageSuppressor => 'Suppressor';

  @override
  String get diagStageVoice => 'Voice detected';

  @override
  String get diagStageGate => 'Gate';

  @override
  String get diagStageLevel => 'Levelling';

  @override
  String get diagStageHiss => 'Hiss';

  @override
  String get website => 'Website';

  @override
  String get openWebsite => 'Open the MumbleWay website';

  @override
  String get helpForThisScreen => 'Help for this screen';

  @override
  String get couldNotOpenLink =>
      'Could not open the link. No browser answered.';

  @override
  String serverRefused(String reason) {
    return 'The server said no: $reason';
  }

  @override
  String get denyText => 'The server would not deliver that message.';

  @override
  String get denyPermission =>
      'The server refused: you do not have permission for that.';

  @override
  String get denySuperUser => 'That account cannot be changed from a client.';

  @override
  String get denyChannelName =>
      'The server would not accept that channel name.';

  @override
  String get denyTextTooLong =>
      'That message is longer than the server allows.';

  @override
  String get denyTemporaryChannel =>
      'That cannot be done in a temporary channel.';

  @override
  String get denyMissingCertificate =>
      'The server needs a certificate for that.';

  @override
  String get denyUserName => 'The server would not accept that name.';

  @override
  String get denyChannelFull => 'That channel is full.';

  @override
  String get denyNestingLimit =>
      'Channels cannot be nested any deeper on this server.';

  @override
  String get denyChannelCountLimit =>
      'The server has as many channels as it allows.';

  @override
  String get denyListenerLimit => 'The server has reached its listener limit.';
}
