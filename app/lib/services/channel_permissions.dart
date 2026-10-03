import '../l10n/app_localizations.dart';

/// The permissions a channel's access list can grant or deny.
///
/// The bit values are Mumble's `ChanACL::Perm`, mirrored from the core's
/// `session::permissions` — this is the half that needs names a rider can read.
///
/// **Only the per-channel ones are here.** Kick, Ban, Register and
/// SelfRegister exist too, and the server reads them from the root channel
/// only; offering them on a sub-channel's rules would be offering something
/// that silently does nothing.
enum ChannelPermission {
  write(0x01),
  traverse(0x02),
  enter(0x04),
  speak(0x08),
  muteDeafen(0x10),
  move(0x20),
  makeChannel(0x40),
  linkChannel(0x80),
  whisper(0x100),
  textMessage(0x200),
  makeTempChannel(0x400),
  listen(0x800);

  const ChannelPermission(this.bit);

  final int bit;

  String label(L l) => switch (this) {
    ChannelPermission.write => l.permWrite,
    ChannelPermission.traverse => l.permTraverse,
    ChannelPermission.enter => l.permEnter,
    ChannelPermission.speak => l.permSpeak,
    ChannelPermission.muteDeafen => l.permMuteDeafen,
    ChannelPermission.move => l.permMove,
    ChannelPermission.makeChannel => l.permMakeChannel,
    ChannelPermission.linkChannel => l.permLinkChannel,
    ChannelPermission.whisper => l.permWhisper,
    ChannelPermission.textMessage => l.permTextMessage,
    ChannelPermission.makeTempChannel => l.permMakeTempChannel,
    ChannelPermission.listen => l.permListen,
  };
}

/// Where one permission stands in a rule.
///
/// Three states rather than two, because a rule carries two masks and a
/// permission can be in neither: *granted*, *denied*, or **not mentioned**,
/// which leaves it to whatever another rule or the parent channel says. A
/// two-state checkbox cannot express the third and would quietly turn silence
/// into a denial.
enum PermissionStand { granted, denied, unset }

PermissionStand standOf(int grant, int deny, ChannelPermission p) {
  if (deny & p.bit != 0) return PermissionStand.denied;
  if (grant & p.bit != 0) return PermissionStand.granted;
  return PermissionStand.unset;
}

/// The next state when the rider taps one: unset → grant → deny → unset.
PermissionStand nextStand(PermissionStand current) => switch (current) {
  PermissionStand.unset => PermissionStand.granted,
  PermissionStand.granted => PermissionStand.denied,
  PermissionStand.denied => PermissionStand.unset,
};

/// Applies a stand to a pair of masks, returning `(grant, deny)`.
(int, int) applyStand(
  int grant,
  int deny,
  ChannelPermission p,
  PermissionStand stand,
) {
  var g = grant & ~p.bit;
  var d = deny & ~p.bit;
  switch (stand) {
    case PermissionStand.granted:
      g |= p.bit;
    case PermissionStand.denied:
      d |= p.bit;
    case PermissionStand.unset:
      break;
  }
  return (g, d);
}

/// A one-line summary of what a rule does, for the list.
String describeRule(L l, int grant, int deny) {
  final granted = [
    for (final p in ChannelPermission.values)
      if (grant & p.bit != 0) p.label(l),
  ];
  final denied = [
    for (final p in ChannelPermission.values)
      if (deny & p.bit != 0) p.label(l),
  ];
  final parts = [
    if (granted.isNotEmpty) l.aclGrants(granted.join(', ')),
    if (denied.isNotEmpty) l.aclDenies(denied.join(', ')),
  ];
  return parts.isEmpty ? l.aclSaysNothing : parts.join(' · ');
}
