import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../widgets/language_button.dart';
import '../widgets/ptt_button.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(L.of(context).settings),
        actions: const [LanguageButton()],
      ),
      body: ListView(
        children: [
          const _SectionHeader('Audio devices'),
          const _DeviceSection(),

          const Divider(height: 32),
          const _SectionHeader('Levels'),
          const _LevelsSection(),

          const Divider(height: 32),
          const _SectionHeader('Noise cancellation'),
          const _Explainer(
            'Filters wind, engine and road noise out of your microphone. '
            'Changes take effect next time the app starts.',
          ),
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
                    title: Text(_noiseTitle(n)),
                    subtitle: Text(_noiseSubtitle(n)),
                    isThreeLine: true,
                  ),
              ],
            ),
          ),

          const Divider(height: 32),
          const _SectionHeader('Microphone mode'),
          const _Explainer(
            'Push-to-talk is the safest choice at speed: nothing you hit on the '
            'road can key the microphone by accident.',
          ),
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
                    title: Text(_micTitle(m)),
                    subtitle: Text(_micSubtitle(m)),
                  ),
              ],
            ),
          ),

          if (state.overlaySupported) ...[
            const Divider(height: 32),
            const _SectionHeader('Floating talk button'),
            const _Explainer(
              'Puts a small draggable push-to-talk button over whatever else '
              'is on screen, with the names of whoever is speaking. Made for '
              'riding with a navigation app in front.',
            ),
            const _OverlayTile(),
          ],

          const Divider(height: 32),
          const _SectionHeader('Network'),
          const _Explainer(
            'Downloads — the public server directory and profile files — go '
            'through the proxy your system is configured with. On a machine '
            'behind one, going direct usually fails outright.',
          ),
          const _ProxyTile(),

          const Divider(height: 32),
          const _SectionHeader('Identity'),
          const _Explainer(
            'Mumble servers recognise you by a certificate this app generated. '
            'Give this fingerprint to a server admin to register your account.',
          ),
          const _FingerprintTile(),
          const SizedBox(height: 32),
        ],
      ),
    );
  }

  static String _noiseTitle(NoiseSetting n) => switch (n) {
        NoiseSetting.off => 'Off',
        NoiseSetting.light => 'Light',
        NoiseSetting.standard => 'Standard',
        NoiseSetting.helmet => 'Helmet / motorcycle',
      };

  static String _noiseSubtitle(NoiseSetting n) => switch (n) {
        NoiseSetting.off => 'No suppression, only a gentle rumble filter.',
        NoiseSetting.light => 'Quiet indoor use; keeps the most natural sound.',
        NoiseSetting.standard => 'General purpose, for most environments.',
        NoiseSetting.helmet =>
          'Steep wind-noise filter, full suppression and an assertive gate. '
              'Built for a microphone inside a helmet at speed.',
      };

  static String _micTitle(MicMode m) => switch (m) {
        MicMode.pushToTalk => 'Push to talk',
        MicMode.voiceActivity => 'Voice activated',
        MicMode.continuous => 'Always on',
      };

  static String _micSubtitle(MicMode m) => switch (m) {
        MicMode.pushToTalk => 'Transmit only while holding the talk button.',
        MicMode.voiceActivity => 'Transmit automatically when you speak.',
        MicMode.continuous => 'Transmit constantly. Uses the most data.',
      };
}

/// Device pickers plus the two test controls.
class _DeviceSection extends StatelessWidget {
  const _DeviceSection();

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    // Desktop hosts enumerate real devices. Phones generally expose a single
    // logical route that the OS switches for you, so there is nothing to pick.
    final canChoose = state.inputDevices.length > 1 ||
        state.outputDevices.length > 1;

    if (!canChoose) {
      return Column(
        children: [
          const _Explainer(
            'This platform routes audio automatically — connecting a headset '
            'switches the app over. Use the system audio settings to choose a '
            'different device.',
          ),
          ListTile(
            leading: const Icon(Icons.refresh),
            title: const Text('Re-check devices'),
            onTap: state.refreshDevices,
          ),
          const _MonitorTile(),
          const _TestOutputTile(),
        ],
      );
    }

    return Column(
      children: [
        _DevicePicker(
          label: 'Microphone',
          icon: Icons.mic_none,
          devices: state.inputDevices,
          selected: state.selectedInput,
          onChanged: state.chooseInputDevice,
        ),
        _DevicePicker(
          label: 'Speakers',
          icon: Icons.speaker,
          devices: state.outputDevices,
          selected: state.selectedOutput,
          onChanged: state.chooseOutputDevice,
        ),
        ListTile(
          leading: const Icon(Icons.refresh),
          title: const Text('Re-check devices'),
          subtitle: const Text('After plugging in or pairing a headset'),
          onTap: state.refreshDevices,
        ),
        const _MonitorTile(),
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
    // `null` is a first-class choice meaning "follow the system default",
    // which is what most users want and what survives replugging a headset.
    final items = <DropdownMenuItem<String?>>[
      const DropdownMenuItem(value: null, child: Text('System default')),
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
                contentPadding:
                    const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
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
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.hearing),
      title: const Text('Test microphone (hear yourself)'),
      subtitle: const Text(
        'Plays your processed voice back, exactly as the far end hears it.',
      ),
      value: state.monitoring,
      onChanged: (_) => state.toggleMonitoring(),
    );
  }
}

class _TestOutputTile extends StatelessWidget {
  const _TestOutputTile();

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    return ListTile(
      leading: const Icon(Icons.volume_up),
      title: const Text('Test speakers'),
      subtitle: const Text('Plays a short tone on the selected output'),
      trailing: FilledButton.tonal(
        onPressed: state.testOutput,
        child: const Text('Play'),
      ),
    );
  }
}

/// Input gain and output volume, with the live meter alongside so the effect
/// of a change is immediately visible.
class _LevelsSection extends StatelessWidget {
  const _LevelsSection();

  @override
  Widget build(BuildContext context) {
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
            label: 'Microphone gain',
            value: state.inputGainDbValue.clamp(minIn, maxIn),
            min: minIn,
            max: maxIn,
            onChanged: state.updateInputGain,
          ),
          _LabelledSlider(
            label: 'Speaker volume',
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

class _ProxyTile extends StatelessWidget {
  const _ProxyTile();

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    return Column(
      children: [
        SwitchListTile(
          secondary: const Icon(Icons.vpn_lock),
          title: const Text('Use the system proxy'),
          subtitle: Text(state.proxyEnabled
              ? state.proxyDescription
              : 'Off — connecting directly'),
          value: state.proxyEnabled,
          onChanged: state.setProxyEnabled,
        ),
        if (state.proxyEnabled)
          ListTile(
            leading: const Icon(Icons.edit_outlined),
            title: const Text('Override proxy'),
            subtitle: Text(state.manualProxy ?? 'Detected automatically'),
            onTap: () => _editOverride(context, state),
          ),
      ],
    );
  }

  Future<void> _editOverride(BuildContext context, AppState state) async {
    final controller = TextEditingController(text: state.manualProxy ?? '');
    final value = await showDialog<String?>(
      context: context,
      builder: (c) => AlertDialog(
        title: const Text('Proxy override'),
        content: TextField(
          controller: controller,
          autofocus: true,
          autocorrect: false,
          decoration: const InputDecoration(
            labelText: 'host:port',
            hintText: '127.0.0.1:8080',
            helperText: 'Leave empty to detect automatically',
          ),
        ),
        actions: [
          TextButton(
              onPressed: () => Navigator.pop(c), child: const Text('Cancel')),
          FilledButton(
            onPressed: () => Navigator.pop(c, controller.text),
            child: const Text('Save'),
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

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: _busy
          ? const SizedBox(
              width: 20, height: 20, child: CircularProgressIndicator(strokeWidth: 2))
          : const Icon(Icons.picture_in_picture_alt),
      title: const Text('Show floating talk button'),
      subtitle: const Text(
        'Needs the "display over other apps" permission.',
      ),
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
    clientCertificateFingerprint().then((v) {
      if (mounted) setState(() => _fp = v);
    }).catchError((Object _) {
      if (mounted) setState(() => _fp = 'unavailable');
    });
  }

  @override
  Widget build(BuildContext context) {
    final fp = _fp;
    return ListTile(
      leading: const Icon(Icons.badge_outlined),
      title: const Text('Certificate fingerprint'),
      subtitle: Text(
        fp ?? 'Loading…',
        style: const TextStyle(fontFamily: 'monospace', fontSize: 11),
      ),
      trailing: fp == null
          ? null
          : IconButton(
              icon: const Icon(Icons.copy),
              tooltip: 'Copy',
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
