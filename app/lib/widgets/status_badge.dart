import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../theme.dart';

/// Maps a connection status onto its colour, icon and label.
///
/// The single place that decides what each state looks like, so every screen
/// agrees. Colour and icon are context-free; the label needs localising, so it
/// is resolved separately rather than baked in.
class StatusVisual {
  const StatusVisual(this.color, this.icon);
  final Color color;
  final IconData icon;

  static StatusVisual of(ConnStatus s) {
    switch (s) {
      case ConnStatus.connected:
        return const StatusVisual(StatusColors.connected, Icons.check_circle);
      case ConnStatus.connecting:
        return const StatusVisual(StatusColors.connecting, Icons.sync);
      case ConnStatus.handshaking:
        return const StatusVisual(StatusColors.connecting, Icons.handshake);
      case ConnStatus.reconnecting:
        return const StatusVisual(
          StatusColors.reconnecting,
          Icons.sync_problem,
        );
      case ConnStatus.failed:
        return const StatusVisual(StatusColors.failed, Icons.error);
      case ConnStatus.disconnected:
        return const StatusVisual(StatusColors.idle, Icons.cloud_off);
      case ConnStatus.idle:
        return const StatusVisual(
          StatusColors.idle,
          Icons.radio_button_unchecked,
        );
    }
  }

  /// Localised name of a status.
  static String labelOf(BuildContext context, ConnStatus s) {
    final l = L.of(context);
    return switch (s) {
      ConnStatus.connected => l.statusConnected,
      ConnStatus.connecting => l.statusConnecting,
      ConnStatus.handshaking => l.statusAuthenticating,
      ConnStatus.reconnecting => l.statusReconnecting,
      ConnStatus.failed => l.statusError,
      ConnStatus.disconnected => l.statusDisconnected,
      ConnStatus.idle => l.statusNotConnected,
    };
  }
}

/// A pill showing connection state. Animates while busy so the user can tell
/// "working on it" from "stuck" without reading the label.
class StatusBadge extends StatelessWidget {
  const StatusBadge({
    super.key,
    required this.status,
    this.detail = '',
    this.compact = false,
  });

  final ConnStatus status;
  final String detail;
  final bool compact;

  bool get _busy =>
      status == ConnStatus.connecting ||
      status == ConnStatus.handshaking ||
      status == ConnStatus.reconnecting;

  @override
  Widget build(BuildContext context) {
    final v = StatusVisual.of(status);
    return Container(
      padding: EdgeInsets.symmetric(
        horizontal: compact ? 10 : 14,
        vertical: compact ? 6 : 8,
      ),
      decoration: BoxDecoration(
        color: v.color.withValues(alpha: 0.16),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: v.color.withValues(alpha: 0.5)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          _Indicator(color: v.color, busy: _busy, icon: v.icon),
          const SizedBox(width: 8),
          Text(
            StatusVisual.labelOf(context, status),
            style: TextStyle(
              color: v.color,
              fontWeight: FontWeight.w700,
              fontSize: compact ? 12 : 14,
            ),
          ),
        ],
      ),
    );
  }
}

class _Indicator extends StatefulWidget {
  const _Indicator({
    required this.color,
    required this.busy,
    required this.icon,
  });
  final Color color;
  final bool busy;
  final IconData icon;

  @override
  State<_Indicator> createState() => _IndicatorState();
}

class _IndicatorState extends State<_Indicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c;

  @override
  void initState() {
    super.initState();
    // Constructed eagerly rather than through a `late final` initialiser: when
    // the badge is not animating nothing reads `_c` until dispose(), and a
    // controller first built during teardown performs an ancestor lookup on an
    // already-deactivated element, which throws.
    _c = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 900),
      value: 1.0,
    );
    if (widget.busy) _c.repeat(reverse: true);
  }

  @override
  void didUpdateWidget(covariant _Indicator old) {
    super.didUpdateWidget(old);
    if (widget.busy && !_c.isAnimating) {
      _c.repeat(reverse: true);
    } else if (!widget.busy && _c.isAnimating) {
      _c.stop();
      _c.value = 1.0;
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return FadeTransition(
      opacity: widget.busy
          ? Tween<double>(begin: 0.35, end: 1.0).animate(_c)
          : const AlwaysStoppedAnimation(1.0),
      child: Icon(widget.icon, size: 16, color: widget.color),
    );
  }
}

/// Shows how voice is travelling: UDP is the low-latency path, TCP means we
/// are tunnelling because UDP is blocked.
class TransportChip extends StatelessWidget {
  const TransportChip({
    super.key,
    required this.transport,
    required this.pingMs,
  });

  final String transport;
  final double pingMs;

  @override
  Widget build(BuildContext context) {
    final udp = transport == 'udp';
    final color = udp ? StatusColors.connected : StatusColors.connecting;
    final ping = pingMs > 0 ? ' · ${pingMs.round()} ms' : '';
    return Tooltip(
      message: udp
          ? 'Direct UDP voice (lowest latency)'
          : 'Tunnelled over TCP because UDP is blocked',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(udp ? Icons.bolt : Icons.vpn_lock, size: 14, color: color),
          const SizedBox(width: 4),
          Text(
            '${udp ? 'UDP' : 'TCP'}$ping',
            style: TextStyle(
              fontSize: 12,
              color: color,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}
