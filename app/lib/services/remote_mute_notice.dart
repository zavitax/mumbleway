import '../l10n/app_localizations.dart';

/// Another MumbleWay rider turned this rider's microphone off or on.
///
/// Only ever a change that **has happened**: the core decides whether to act on
/// a request — the rider's setting, a cooldown, whether the microphone was
/// already that way — and a request it declined never reaches Dart. A notice
/// about a change that did not happen would tell a rider their microphone is
/// in a state it is not in.
///
/// The cue is the half of this that matters on a bike; it has already played
/// by the time this exists. This is the half for a rider who glances down to
/// find out who.
class RemoteMuteNotice {
  const RemoteMuteNotice({
    required this.serverId,
    required this.muted,
    required this.by,
  });

  final String serverId;

  /// What the microphone is now: `true` for turned off.
  final bool muted;

  /// The other rider's name, as the server reports it.
  final String by;

  String describe(L l) => muted ? l.remoteMutedYou(by) : l.remoteUnmutedYou(by);
}
