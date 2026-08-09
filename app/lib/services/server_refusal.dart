import '../l10n/app_localizations.dart';

/// The server saying no.
///
/// A Mumble server answers a refused action with `PermissionDenied`, and what
/// it puts in it varies: some send a sentence, most send only a type. Both have
/// to end up as something a rider can read, in their own language, which is why
/// the type is carried rather than a message invented in the core.
class ServerRefusal {
  const ServerRefusal({
    required this.serverId,
    required this.reason,
    required this.kind,
  });

  final String serverId;

  /// The server's own words, often empty.
  final String reason;

  /// Mumble's `DenyType`. The numbers are protocol, not ours.
  final int kind;

  /// What to show the user.
  ///
  /// The server's own sentence wins when there is one: it is more specific than
  /// anything derivable from a type, and an admin who wrote a custom message
  /// meant it to be read. Otherwise the type is translated, and an unknown type
  /// still says that permission was refused rather than nothing at all — new
  /// deny types are added to Mumble from time to time and a client that fell
  /// silent on them would be worse than one that is vague.
  String describe(L l) {
    final own = reason.trim();
    if (own.isNotEmpty) return own;
    return switch (kind) {
      0 => l.denyText,
      1 => l.denyPermission,
      2 => l.denySuperUser,
      3 => l.denyChannelName,
      4 => l.denyTextTooLong,
      6 => l.denyTemporaryChannel,
      7 => l.denyMissingCertificate,
      8 => l.denyUserName,
      9 => l.denyChannelFull,
      10 => l.denyNestingLimit,
      11 => l.denyChannelCountLimit,
      12 => l.denyListenerLimit,
      13 => l.denyListenerLimit,
      _ => l.denyPermission,
    };
  }
}
