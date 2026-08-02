import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../services/button_controller.dart';
import '../services/overlay.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/language_button.dart';
import '../widgets/ptt_button.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final l = L.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(l.settings),
        actions: const [LanguageButton()],
      ),
      body: ListView(
        children: [
          _SectionHeader(l.audioDevices),
          const _DeviceSection(),

          const Divider(height: 32),
          _SectionHeader(l.levels),
          const _LevelsSection(),
          const _GlitchCounters(),

          const Divider(height: 32),
          _SectionHeader(l.noiseCancellation),
          _Explainer(l.noiseCancellationBody),
          RadioGroup<NoiseSetting>(
            groupValue: state.noise,
            onChanged: (v) {
              if (v != null) state.updateNoise(v);
            },
            child: Column(
              children: [
                for (final n in NoiseSetting.values)
                  RadioListTile<NoiseSetting>(
                    value: n,
                    title: Text(_noiseTitle(l, n)),
                    subtitle: Text(_noiseSubtitle(l, n)),
                    isThreeLine: true,
                  ),
              ],
            ),
          ),

          const Divider(height: 32),
          _SectionHeader(l.micMode),
          _Explainer(l.micModeBody),
          RadioGroup<MicMode>(
            groupValue: state.micMode,
            onChanged: (v) {
              if (v != null) state.updateMicMode(v);
            },
            child: Column(
              children: [
                for (final m in MicMode.values)
                  RadioListTile<MicMode>(
                    value: m,
                    title: Text(_micTitle(l, m)),
                    subtitle: Text(_micSubtitle(l, m)),
                  ),
              ],
            ),
          ),

          if (state.overlaySupported) ...[
            const Divider(height: 32),
            _SectionHeader(l.floatingTalkButton),
            _Explainer(l.floatingTalkButtonBody),
            const _OverlayTile(),
          ],

          const Divider(height: 32),
          _SectionHeader(l.buttons),
          _Explainer(l.buttonsBody),
          const _ButtonBindings(),

          const Divider(height: 32),
          _SectionHeader(l.network),
          _Explainer(l.networkBody),
          const _ProxyTile(),

          const Divider(height: 32),
          _SectionHeader(l.identity),
          _Explainer(l.identityBody),
          const _FingerprintTile(),
          const SizedBox(height: 32),
        ],
      ),
    );
  }

  static String _noiseTitle(L l, NoiseSetting n) => switch (n) {
    NoiseSetting.off => l.noiseOff,
    NoiseSetting.light => l.noiseLight,
    NoiseSetting.standard => l.noiseStandard,
    NoiseSetting.helmet => l.noiseHelmet,
  };

  static String _noiseSubtitle(L l, NoiseSetting n) => switch (n) {
    NoiseSetting.off => l.noiseOffBody,
    NoiseSetting.light => l.noiseLightBody,
    NoiseSetting.standard => l.noiseStandardBody,
    NoiseSetting.helmet => l.noiseHelmetBody,
  };

  static String _micTitle(L l, MicMode m) => switch (m) {
    MicMode.pushToTalk => l.micPushToTalk,
    MicMode.voiceActivity => l.micVoiceActivated,
    MicMode.continuous => l.micAlwaysOn,
  };

  static String _micSubtitle(L l, MicMode m) => switch (m) {
    MicMode.pushToTalk => l.micPushToTalkBody,
    MicMode.voiceActivity => l.micVoiceActivatedBody,
    MicMode.continuous => l.micAlwaysOnBody,
  };
}

/// Device pickers plus the two test controls.
class _DeviceSection extends StatelessWidget {
  const _DeviceSection();

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final l = L.of(context);

    // Desktop hosts enumerate real devices. Phones generally expose a single
    // logical route that the OS switches for you, so there is nothing to pick.
    final canChoose =
        state.inputDevices.length > 1 || state.outputDevices.length > 1;

    if (!canChoose) {
      return Column(
        children: [
          _Explainer(l.platformRoutesAudio),
          ListTile(
            leading: const Icon(Icons.refresh),
            title: Text(l.recheckDevices),
            onTap: state.refreshDevices,
          ),
          const _MonitorTile(),
          const _EchoCancellationTile(),
          const _NormaliseTile(),
          const _TestOutputTile(),
        ],
      );
    }

    return Column(
      children: [
        _DevicePicker(
          label: l.microphone,
          icon: Icons.mic_none,
          devices: state.inputDevices,
          selected: state.selectedInput,
          onChanged: state.chooseInputDevice,
        ),
        _DevicePicker(
          label: l.speakers,
          icon: Icons.speaker,
          devices: state.outputDevices,
          selected: state.selectedOutput,
          onChanged: state.chooseOutputDevice,
        ),
        ListTile(
          leading: const Icon(Icons.refresh),
          title: Text(l.recheckDevices),
          subtitle: Text(l.recheckDevicesBody),
          onTap: state.refreshDevices,
        ),
        const _MonitorTile(),
        const _EchoCancellationTile(),
        const _NormaliseTile(),
        const _TestOutputTile(),
      ],
    );
  }
}

class _DevicePicker extends StatelessWidget {
  const _DevicePicker({
    required this.label,
    required this.icon,
    required this.devices,
    required this.selected,
    required this.onChanged,
  });

  final String label;
  final IconData icon;
  final List<String> devices;
  final String? selected;
  final ValueChanged<String?> onChanged;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    // `null` is a first-class choice meaning "follow the system default",
    // which is what most users want and what survives replugging a headset.
    final items = <DropdownMenuItem<String?>>[
      DropdownMenuItem(value: null, child: Text(l.systemDefault)),
      for (final d in devices)
        DropdownMenuItem(
          value: d,
          child: Text(d, overflow: TextOverflow.ellipsis),
        ),
    ];

    final value = devices.contains(selected) ? selected : null;

    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 6, 20, 6),
      child: Row(
        children: [
          Icon(icon, size: 20),
          const SizedBox(width: 14),
          Expanded(
            child: DropdownButtonFormField<String?>(
              initialValue: value,
              isExpanded: true,
              decoration: InputDecoration(
                labelText: label,
                contentPadding: const EdgeInsets.symmetric(
                  horizontal: 14,
                  vertical: 10,
                ),
              ),
              items: items,
              onChanged: onChanged,
            ),
          ),
        ],
      ),
    );
  }
}

class _MonitorTile extends StatelessWidget {
  const _MonitorTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.hearing),
      title: Text(l.testMicrophone),
      subtitle: const Text(
        'Plays your processed voice back, exactly as the far end hears it.',
      ),
      value: state.monitoring,
      onChanged: (_) => state.toggleMonitoring(),
    );
  }
}

class _NormaliseTile extends StatelessWidget {
  const _NormaliseTile();

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.equalizer),
      title: const Text('Even out speaker loudness'),
      subtitle: const Text(
        'Brings everyone to a similar level. Adapts on what it hears, so if '
        'a hiss rises between sentences, turn this off to check.',
      ),
      isThreeLine: true,
      value: state.normaliseLevels,
      onChanged: (v) => state.setNormaliseLevels(value: v),
    );
  }
}

class _EchoCancellationTile extends StatelessWidget {
  const _EchoCancellationTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.surround_sound),
      title: Text(l.echoCancellation),
      subtitle: Text(l.echoCancellationBody),
      isThreeLine: true,
      value: state.echoCancellation,
      onChanged: (v) => state.setEchoCancellationEnabled(value: v),
    );
  }
}

class _TestOutputTile extends StatelessWidget {
  const _TestOutputTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return ListTile(
      leading: const Icon(Icons.volume_up),
      title: Text(l.testSpeakers),
      subtitle: Text(l.testSpeakersBody),
      trailing: FilledButton.tonal(
        onPressed: state.testOutput,
        child: Text(l.play),
      ),
    );
  }
}

/// Input gain and output volume, with the live meter alongside so the effect
/// of a change is immediately visible.
/// Live dropout counters.
///
/// Choppy audio sounds identical whatever causes it, and the three candidates
/// need different fixes: the playback queue running dry, the microphone
/// outrunning the processing, or gaps that were already in the stream when it
/// arrived. Two numbers tell them apart in seconds, where listening cannot.
class _GlitchCounters extends StatefulWidget {
  const _GlitchCounters();

  @override
  State<_GlitchCounters> createState() => _GlitchCountersState();
}

class _GlitchCountersState extends State<_GlitchCounters> {
  Timer? _tick;
  List<int> _ms = const [0, 0];
  List<int> _incoming = const [0, 0];

  @override
  void initState() {
    super.initState();
    _tick = Timer.periodic(const Duration(milliseconds: 500), (_) {
      if (!mounted) return;
      try {
        // u64 crosses the bridge as BigInt; these are millisecond counts.
        setState(() {
          _ms = audioGlitchMs().map((v) => v.toInt()).toList();
          _incoming = incomingAudioMs().map((v) => v.toInt()).toList();
        });
      } catch (_) {
        // The engine is not up; nothing to report.
      }
    });
  }

  @override
  void dispose() {
    _tick?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final quiet = Theme.of(context).colorScheme.onSurfaceVariant;
    final bad = _ms[0] > 0 || _ms[1] > 0;
    return ListTile(
      dense: true,
      leading: Icon(
        bad ? Icons.warning_amber : Icons.check_circle_outline,
        size: 20,
        color: bad ? StatusColors.connecting : quiet,
      ),
      title: Text(
        'Playback gaps ${_ms[0]} ms · microphone dropped ${_ms[1]} ms\n'
        'Incoming: ${_incoming[1]} ms real · ${_incoming[0]} ms invented',
        style: const TextStyle(fontSize: 12),
      ),
      isThreeLine: true,
      trailing: TextButton(
        onPressed: () {
          resetAudioGlitches();
          setState(() => _ms = const [0, 0]);
        },
        child: const Text('Reset'),
      ),
    );
  }
}

class _LevelsSection extends StatelessWidget {
  const _LevelsSection();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final minIn = state.gainRange[0];
    final maxIn = state.gainRange[1];
    final minOut = state.gainRange[2];
    final maxOut = state.gainRange[3];

    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          const Padding(
            padding: EdgeInsets.only(bottom: 10),
            child: LevelMeter(showScale: true),
          ),
          _LabelledSlider(
            label: l.microphoneGain,
            value: state.inputGainDbValue.clamp(minIn, maxIn),
            min: minIn,
            max: maxIn,
            onChanged: state.updateInputGain,
          ),
          _LabelledSlider(
            label: l.speakerVolume,
            value: state.outputVolumeDbValue.clamp(minOut, maxOut),
            min: minOut,
            max: maxOut,
            onChanged: state.updateOutputVolume,
          ),
          const SizedBox(height: 4),
          Text(
            'Aim for the meter to peak around three quarters while speaking '
            'normally. Too much gain lifts the engine noise with your voice.',
            style: TextStyle(
              fontSize: 11,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 12),
        ],
      ),
    );
  }
}

class _LabelledSlider extends StatelessWidget {
  const _LabelledSlider({
    required this.label,
    required this.value,
    required this.min,
    required this.max,
    required this.onChanged,
  });

  final String label;
  final double value;
  final double min;
  final double max;
  final ValueChanged<double> onChanged;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Expanded(child: Text(label, style: const TextStyle(fontSize: 13))),
            Text(
              '${value >= 0 ? '+' : ''}${value.toStringAsFixed(0)} dB',
              style: const TextStyle(
                fontSize: 12,
                fontFeatures: [FontFeature.tabularFigures()],
              ),
            ),
          ],
        ),
        Slider(
          value: value,
          min: min,
          max: max,
          divisions: (max - min).round(),
          onChanged: onChanged,
        ),
      ],
    );
  }
}

/// Lists bound buttons and learns new ones.
///
/// Learning captures whatever the device sends rather than offering a list of
/// keys: Bluetooth remotes report all sorts of codes, including ones Flutter
/// has no name for, and the only reliable way to know what a given remote
/// sends is to press it.
class _ButtonBindings extends StatefulWidget {
  const _ButtonBindings();

  @override
  State<_ButtonBindings> createState() => _ButtonBindingsState();
}

class _ButtonBindingsState extends State<_ButtonBindings> {
  ButtonAction _action = ButtonAction.pushToTalk;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final learning = state.buttons.isLearning;

    return Column(
      children: [
        for (final b in state.buttonBindings)
          ListTile(
            leading: const Icon(Icons.radio_button_checked),
            title: Text(b.displayName),
            subtitle: Text(b.action.label),
            trailing: IconButton(
              icon: const Icon(Icons.close),
              tooltip: l.removeBinding,
              onPressed: () => state.removeButtonBinding(b.keyId),
            ),
          ),
        if (state.buttonBindings.isEmpty)
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 4),
            child: Text(l.noButtonsBound, style: const TextStyle(fontSize: 12)),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 12, 20, 4),
          child: Row(
            children: [
              Expanded(
                child: DropdownButtonFormField<ButtonAction>(
                  initialValue: _action,
                  isExpanded: true,
                  decoration: InputDecoration(
                    labelText: l.action,
                    contentPadding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 10,
                    ),
                  ),
                  items: [
                    for (final a in ButtonAction.values)
                      DropdownMenuItem(value: a, child: Text(a.label)),
                  ],
                  onChanged: (v) => setState(() => _action = v ?? _action),
                ),
              ),
              const SizedBox(width: 10),
              FilledButton.tonal(
                onPressed: learning
                    ? state.cancelLearningButton
                    : () => state.learnButton(_action, (b) {
                        if (!context.mounted) return;
                        ScaffoldMessenger.of(context).showSnackBar(
                          SnackBar(content: Text(l.boundButton(b.displayName))),
                        );
                      }),
                child: Text(learning ? l.cancel : l.learn),
              ),
            ],
          ),
        ),
        if (learning)
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 4, 20, 12),
            child: Row(
              children: [
                const SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    l.pressButtonNow,
                    style: const TextStyle(fontSize: 12),
                  ),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

class _ProxyTile extends StatelessWidget {
  const _ProxyTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return Column(
      children: [
        SwitchListTile(
          secondary: const Icon(Icons.vpn_lock),
          title: Text(l.useSystemProxy),
          subtitle: Text(
            state.proxyEnabled ? state.proxyDescription : l.proxyOffDirect,
          ),
          value: state.proxyEnabled,
          onChanged: state.setProxyEnabled,
        ),
        if (state.proxyEnabled)
          ListTile(
            leading: const Icon(Icons.edit_outlined),
            title: Text(l.overrideProxy),
            subtitle: Text(state.manualProxy ?? l.detectedAutomatically),
            onTap: () => _editOverride(context, state),
          ),
      ],
    );
  }

  Future<void> _editOverride(BuildContext context, AppState state) async {
    final l = L.of(context);
    final controller = TextEditingController(text: state.manualProxy ?? '');
    final value = await showDialog<String?>(
      context: context,
      builder: (c) => AlertDialog(
        title: Text(l.proxyOverride),
        content: TextField(
          controller: controller,
          autofocus: true,
          autocorrect: false,
          decoration: InputDecoration(
            labelText: l.proxyHostPort,
            hintText: l.proxyHostPortHint,
            helperText: l.proxyAutoDetect,
          ),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(c), child: Text(l.cancel)),
          FilledButton(
            onPressed: () => Navigator.pop(c, controller.text),
            child: Text(l.save),
          ),
        ],
      ),
    );
    controller.dispose();
    if (value != null) await state.setManualProxy(value);
  }
}

class _OverlayTile extends StatefulWidget {
  const _OverlayTile();

  @override
  State<_OverlayTile> createState() => _OverlayTileState();
}

class _OverlayTileState extends State<_OverlayTile> {
  bool _busy = false;

  /// Each platform grants a different set of controls, and on iOS the system
  /// owns the buttons outright. Saying which is which up front is the only way
  /// the mapping is ever discoverable — there is nowhere to label them.
  String _subtitleFor(FloatingKind kind) {
    switch (kind) {
      case FloatingKind.androidOverlay:
        return 'Talk, mute, deafen and hang up over other apps. '
            'Needs the "display over other apps" permission.';
      case FloatingKind.iosPictureInPicture:
        return 'Picture in Picture, appearing when you leave the app. '
            'The system allows three buttons: play/pause talks, '
            'skip back mutes, skip forward hangs up (twice to confirm).';
      case FloatingKind.macosPanel:
        return 'A small always-on-top panel with talk, mute, deafen '
            'and hang up.';
      case FloatingKind.none:
        return 'Not available on this platform.';
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final kind = state.overlayKind;
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        SwitchListTile(
          secondary: _busy
              ? const SizedBox(
                  width: 20,
                  height: 20,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : const Icon(Icons.picture_in_picture_alt),
          title: Text(l.floatingWindow),
          subtitle: Text(_subtitleFor(kind)),
          isThreeLine: kind == FloatingKind.iosPictureInPicture,
          value: state.overlayEnabled,
          onChanged: _busy
              ? null
              : (want) async {
                  setState(() => _busy = true);
                  final messenger = ScaffoldMessenger.of(context);
                  String? error;
                  if (want) {
                    error = await state.enableOverlay();
                  } else {
                    await state.disableOverlay();
                  }
                  if (!mounted) return;
                  setState(() => _busy = false);
                  if (error != null) {
                    messenger.showSnackBar(SnackBar(content: Text(error)));
                  }
                },
        ),
      ],
    );
  }
}

class _FingerprintTile extends StatefulWidget {
  const _FingerprintTile();

  @override
  State<_FingerprintTile> createState() => _FingerprintTileState();
}

class _FingerprintTileState extends State<_FingerprintTile> {
  String? _fp;

  @override
  void initState() {
    super.initState();
    clientCertificateFingerprint()
        .then((v) {
          if (mounted) setState(() => _fp = v);
        })
        .catchError((Object _) {
          if (mounted) setState(() => _fp = 'unavailable');
        });
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final fp = _fp;
    return ListTile(
      leading: const Icon(Icons.badge_outlined),
      title: Text(l.certificateFingerprint),
      subtitle: Text(
        fp ?? 'Loading…',
        style: const TextStyle(fontFamily: 'monospace', fontSize: 11),
      ),
      trailing: fp == null
          ? null
          : IconButton(
              icon: const Icon(Icons.copy),
              tooltip: l.copy,
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: fp));
                if (!context.mounted) return;
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Fingerprint copied')),
                );
              },
            ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 20, 20, 4),
      child: Text(
        text.toUpperCase(),
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.w800,
          letterSpacing: 1.1,
          color: Theme.of(context).colorScheme.primary,
        ),
      ),
    );
  }
}

class _Explainer extends StatelessWidget {
  const _Explainer(this.text);
  final String text;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 0, 20, 10),
      child: Text(
        text,
        style: TextStyle(
          fontSize: 12,
          color: Theme.of(context).colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}
