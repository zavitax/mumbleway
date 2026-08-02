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
