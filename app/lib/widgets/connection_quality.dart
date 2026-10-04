import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../theme.dart';

/// How a connection is doing, in the three answers a rider can act on.
enum LinkGrade {
  /// Nothing to say about it.
  good,

  /// Audible if you listen for it: the odd word clipped, or a delay that makes
  /// people talk over each other.
  fair,

  /// Breaking up. Worth telling them.
  poor,
}

/// Grades a rider's connection from the server's own measurements.
///
/// **Loss decides it before ping does, and the worse direction wins.** A rider
/// losing a tenth of what they send is unintelligible however fast the round
/// trip is, and losing what the server sends *them* is the same fault pointed
/// the other way — they will be answering questions nobody asked them.
///
/// The thresholds are about speech, not about networks. Opus rides out the odd
/// missing packet, so a couple of per cent is ordinary and unremarkable; near a
/// tenth, syllables go. A round trip under about 120 ms is a normal
/// conversation; past 300 ms people start talking over each other, which is
/// worth flagging even when nothing is lost at all.
LinkGrade gradeFor(UiQuality q) {
  final loss = q.lossUp > q.lossDown ? q.lossUp : q.lossDown;
  if (loss >= 0.08 || q.pingMs >= 300) return LinkGrade.poor;
  if (loss >= 0.02 || q.pingMs >= 120) return LinkGrade.fair;
  return LinkGrade.good;
}

/// A rider's connection, as bars beside their name.
///
/// **Always drawn, including when there is nothing to draw.** A row that
/// simply loses the bars is a row that has changed shape, and the eye reads
/// that as a connection having *gone* rather than as a measurement not having
/// arrived — which is the ordinary case for anybody outside our own channel,
/// since that is all the server measures. Unmeasured is the grey placeholder
/// at half opacity: present, quiet, and plainly saying nothing.
///
/// The healthy state is deliberately quiet too: full bars at a third opacity,
/// so a channel where everybody is fine has no colour in it and the one rider
/// who is struggling is the only thing the eye finds.
class ConnectionQualityBars extends StatelessWidget {
  const ConnectionQualityBars({
    super.key,
    required this.quality,
    this.size = 14,
  });

  /// `null` when the server has not measured this rider.
  final UiQuality? quality;

  /// Drawn at the size of whatever it stands beside, so it reads as one of the
  /// row's controls rather than as a mark on the name.
  final double size;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final measured = quality;
    if (measured == null) {
      return Tooltip(
        message: l.qualityUnmeasured,
        triggerMode: TooltipTriggerMode.tap,
        child: Opacity(
          opacity: 0.5,
          child: Icon(
            Icons.signal_cellular_alt,
            size: size,
            color: Theme.of(context).colorScheme.onSurfaceVariant,
          ),
        ),
      );
    }
    final grade = gradeFor(measured);
    final (icon, colour) = switch (grade) {
      LinkGrade.good => (Icons.signal_cellular_alt, StatusColors.connected),
      LinkGrade.fair => (
        Icons.signal_cellular_alt_2_bar,
        StatusColors.connecting,
      ),
      LinkGrade.poor => (Icons.signal_cellular_alt_1_bar, StatusColors.failed),
    };

    return Tooltip(
      message: describeQuality(l, measured),
      child: Icon(
        icon,
        size: size,
        color: grade == LinkGrade.good
            ? colour.withValues(alpha: 0.35)
            : colour,
      ),
    );
  }
}

/// What the bars say when a rider asks them.
///
/// Both directions are named from the rider's point of view — *their voice* and
/// *what they hear* — rather than as "up" and "down", which say nothing about
/// who is about to be misunderstood.
String describeQuality(L l, UiQuality q) {
  final ping = q.udp
      ? l.qualityPing(q.pingMs.round())
      : l.qualityPingTunnelled(q.pingMs.round());
  final window = q.windowSecs > 0
      ? l.qualityWindow(q.windowSecs)
      : l.qualitySinceConnect;
  return '$ping\n'
      '${l.qualityLossUp(_percent(q.lossUp))}\n'
      '${l.qualityLossDown(_percent(q.lossDown))}\n'
      '$window';
}

/// Loss as a whole number of per cent.
///
/// Rounded *up* away from zero, so a connection that is losing something never
/// reports "0% lost" — a figure a rider would read as nothing being wrong while
/// hearing otherwise.
int _percent(double fraction) {
  final pct = fraction * 100;
  if (pct <= 0) return 0;
  return pct < 1 ? 1 : pct.round();
}
