import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';

class SettingsScreen extends StatelessWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);

    return Scaffold(
      appBar: AppBar(title: const Text('Settings')),
      body: ListView(
        children: [
          const _SectionHeader('Noise cancellation'),
          _Explainer(
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
          _Explainer(
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

          const Divider(height: 32),
          const _SectionHeader('Identity'),
          _Explainer(
            'Mumble servers recognise you by a certificate this app generated. '
            'Give this fingerprint to a server admin to register your account.',
          ),
          const _FingerprintTile(),

          const Divider(height: 32),
          const _SectionHeader('Audio devices'),
          const _DeviceList(),
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

class _DeviceList extends StatefulWidget {
  const _DeviceList();

  @override
  State<_DeviceList> createState() => _DeviceListState();
}

class _DeviceListState extends State<_DeviceList> {
  List<String> _inputs = const [];
  List<String> _outputs = const [];

  @override
  void initState() {
    super.initState();
    _load();
  }

  Future<void> _load() async {
    try {
      final i = await audioInputDevices();
      final o = await audioOutputDevices();
      if (mounted) {
        setState(() {
          _inputs = i;
          _outputs = o;
        });
      }
    } catch (_) {
      // Device enumeration can fail on a locked-down platform; not fatal.
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ListTile(
          leading: const Icon(Icons.mic_none),
          title: const Text('Inputs'),
          subtitle: Text(_inputs.isEmpty ? 'None found' : _inputs.join('\n')),
        ),
        ListTile(
          leading: const Icon(Icons.speaker),
          title: const Text('Outputs'),
          subtitle: Text(_outputs.isEmpty ? 'None found' : _outputs.join('\n')),
        ),
      ],
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
