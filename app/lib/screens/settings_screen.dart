import 'dart:async';

import 'package:flutter/material.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../services/button_controller.dart';
import '../services/cloud_sync.dart';
import '../services/noise_profiles.dart';
import '../services/overlay.dart';
import '../services/proxy.dart';
import '../services/site_links.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/language_button.dart';
import '../widgets/ptt_button.dart';

/// The audio settings, which hold the microphone open for as long as they are
/// on screen.
///
/// Every control here is about a signal: the gain slider, the noise profile,
/// the activation threshold, the de-hisser. Judging any of them means watching
/// the meter move while you talk, so the devices are opened on the way in
/// rather than waiting for the microphone test to be switched on — which would
/// leave the meter above the gain slider dead at exactly the moment somebody
/// is dragging it.
///
/// Released on the way out, unless a call or the microphone test still wants
/// them. Nothing else in the app holds them just for being looked at.
class SettingsScreen extends StatefulWidget {
  const SettingsScreen({super.key});

  @override
  State<SettingsScreen> createState() => _SettingsScreenState();
}

class _SettingsScreenState extends State<SettingsScreen> {
  AppState? _held;

  /// Why the microphone could not be opened, if it could not.
  ///
  /// Shown where the meter would have been. A dead meter with no explanation
  /// reads as a broken app; the reason is usually another application holding
  /// the device, which is something the user can go and undo.
  String? _audioError;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Not initState: the state object is reached through an inherited widget,
    // which is not available that early. Guarded because this runs again on
    // every dependency change, and a second hold would never be released.
    if (_held != null) return;
    final state = AppStateScope.of(context);
    _held = state;
    unawaited(
      state.holdAudio().then((error) {
        if (mounted) setState(() => _audioError = error);
      }),
    );
  }

  @override
  void dispose() {
    _held?.releaseAudio();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = AppStateScope.of(context);
    final l = L.of(context);

    return Scaffold(
      appBar: AppBar(
        title: Text(l.settings),
        // Help last, so it sits at the outside edge and stays put as other
        // actions come and go.
        actions: [
          const LanguageButton(),
          IconButton(
            tooltip: l.helpForThisScreen,
            onPressed: () =>
                openSite(context, SiteLinks.settings(siteLanguage(context))),
            icon: const Icon(Icons.help_outline),
          ),
        ],
      ),
      body: ListView(
        children: [
          _SectionHeader(l.audioDevices),
          const _DeviceSection(),

          const Divider(height: 32),
          _SectionHeader(l.levels),
          _LevelsSection(audioError: _audioError),

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
          _SectionHeader(l.feedbackGuard),
          _Explainer(l.feedbackGuardBody),
          RadioGroup<FeedbackGuardMode>(
            groupValue: state.feedbackGuard,
            onChanged: (v) {
              if (v != null) state.updateFeedbackGuard(v);
            },
            child: Column(
              children: [
                for (final m in FeedbackGuardMode.values)
                  RadioListTile<FeedbackGuardMode>(
                    value: m,
                    title: Text(_feedbackTitle(l, m)),
                    subtitle: Text(_feedbackSubtitle(l, m)),
                    isThreeLine: true,
                  ),
              ],
            ),
          ),

          const Divider(height: 32),
          _SectionHeader(l.dehiss),
          _Explainer(l.dehissBody),
          RadioGroup<DehissOption>(
            groupValue: state.dehiss,
            onChanged: (v) {
              if (v != null) state.updateDehiss(v);
            },
            child: Column(
              children: [
                for (final m in DehissOption.values)
                  RadioListTile<DehissOption>(
                    value: m,
                    title: Text(_dehissTitle(l, m)),
                    subtitle: Text(_dehissSubtitle(l, m)),
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
            _SectionHeader(l.floatingCallWindow),
            _Explainer(l.floatingCallWindowBody),
            const _OverlayTile(),
          ],

          const Divider(height: 32),
          _SectionHeader(l.buttons),
          _Explainer(l.buttonsBody),
          // Only where it is true. Android's media session reports a press and
          // a release like any other key, so the limitation is Apple's alone.
          if (state.remoteButtonsAreTapsOnly) _Explainer(l.buttonsIosNote),
          const _ButtonBindings(),
          const _ButtonDiagnostics(),

          const Divider(height: 32),
          _SectionHeader(l.network),
          _Explainer(l.networkBody),
          const _ProxyTile(),

          // Only where something can actually carry the data to another
          // device. On Windows there is nothing behind this at all, and a
          // permanently greyed row explaining an absence is worse than no row:
          // it reads as a feature that is broken rather than one that was never
          // offered, and it is the sort of thing a user taps at repeatedly.
          if (state.cloudKind == CloudKind.icloud) ...[
            const Divider(height: 32),
            _SectionHeader(l.syncTitle),
            const _SyncTile(),
          ],

          const Divider(height: 32),
          _SectionHeader(l.identity),
          _Explainer(l.identityBody),
          const _FingerprintTile(),
          const SizedBox(height: 28),
          const _VersionFooter(),
          const SizedBox(height: 28),
        ],
      ),
    );
  }

  static String _noiseTitle(L l, NoiseSetting n) => noiseProfileTitle(l, n);

  static String _noiseSubtitle(L l, NoiseSetting n) => switch (n) {
    NoiseSetting.off => l.noiseOffBody,
    NoiseSetting.light => l.noiseLightBody,
    NoiseSetting.standard => l.noiseStandardBody,
    NoiseSetting.helmet => l.noiseHelmetBody,
    NoiseSetting.auto => l.noiseAutoBody,
  };

  static String _feedbackTitle(L l, FeedbackGuardMode m) => switch (m) {
    FeedbackGuardMode.off => l.feedbackOff,
    FeedbackGuardMode.duck => l.feedbackDuck,
    FeedbackGuardMode.howlGuard => l.feedbackHowl,
    FeedbackGuardMode.residual => l.feedbackResidual,
  };

  static String _feedbackSubtitle(L l, FeedbackGuardMode m) => switch (m) {
    FeedbackGuardMode.off => l.feedbackOffBody,
    FeedbackGuardMode.duck => l.feedbackDuckBody,
    FeedbackGuardMode.howlGuard => l.feedbackHowlBody,
    FeedbackGuardMode.residual => l.feedbackResidualBody,
  };

  static String _dehissTitle(L l, DehissOption m) => switch (m) {
    DehissOption.off => l.dehissOff,
    DehissOption.expander => l.dehissExpander,
    DehissOption.spectral => l.dehissSpectral,
  };

  static String _dehissSubtitle(L l, DehissOption m) => switch (m) {
    DehissOption.off => l.dehissOffBody,
    DehissOption.expander => l.dehissExpanderBody,
    DehissOption.spectral => l.dehissSpectralBody,
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

    // Two different situations that look identical if you only count devices.
    //
    // A phone exposes one logical route and switches it itself as headsets
    // come and go: there is nothing to pick, and nothing a re-check could
    // turn up, so offering the button puts a control on screen that cannot do
    // anything. A desktop with one device is not that — plug in a headset and
    // the list really does change — so there the button is the whole point.
    final routesItself = !state.canPickAudioDevices;
    final canChoose =
        state.inputDevices.length > 1 || state.outputDevices.length > 1;

    return Column(
      children: [
        if (routesItself) _Explainer(l.platformRoutesAudio),
        if (!routesItself) ...[
          if (canChoose) ...[
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
          ],
          ListTile(
            leading: const Icon(Icons.refresh),
            title: Text(l.recheckDevices),
            subtitle: Text(l.recheckDevicesBody),
            onTap: state.refreshDevices,
          ),
        ],
        const _MonitorTile(),
        const _EchoCancellationTile(),
        const _SimpleModelTile(),
        const _NormaliseTile(),
        const _JitterBufferTile(),
        const _ReverbTile(),
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
      subtitle: Text(l.testMicrophoneBody),
      value: state.monitoring,
      onChanged: (_) async {
        // Captured before the await: the switch can be gone by the time the
        // microphone answers, and this is the only place the reason appears.
        final messenger = ScaffoldMessenger.of(context);
        final error = await state.toggleMonitoring();
        if (error != null) {
          messenger.showSnackBar(SnackBar(content: Text(error)));
        }
      },
    );
  }
}

/// Syncing the server list, and what that means on this platform.
///
/// Three sentences rather than one switch, because the facilities behind them
/// are genuinely different and a user who assumes iCloud behaviour on Android
/// will believe their phone is broken when the laptop does not catch up.
class _SyncTile extends StatelessWidget {
  const _SyncTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);

    switch (state.cloudKind) {
      // Both unreachable: the caller shows this section only where something
      // genuinely keeps two devices in step. Android's backup restores to a new
      // phone and never syncs between two that are in use; Windows has nothing
      // at all. Describing either under a heading called "Sync" promises
      // something that will not happen. Kept because the switch must be
      // exhaustive, and drawing nothing is the right answer by any other route.
      case CloudKind.none:
      case CloudKind.androidBackup:
        return const SizedBox.shrink();

      case CloudKind.icloud:
        final error = state.cloudError;
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            SwitchListTile(
              secondary: Icon(
                state.cloudSync ? Icons.cloud_sync : Icons.cloud_off,
              ),
              title: Text(l.syncServers),
              subtitle: Text(
                state.cloudReady ? l.syncBodyICloud : l.syncSignedOut,
              ),
              isThreeLine: true,
              value: state.cloudSync,
              onChanged: (v) => state.setCloudSync(v),
            ),
            if (state.cloudSync && error != null)
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: Text(
                  l.syncFailed(error),
                  style: const TextStyle(
                    fontSize: 11,
                    color: StatusColors.failed,
                  ),
                ),
              ),
            if (state.cloudSync && state.cloudReady)
              Align(
                alignment: Alignment.centerLeft,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                  child: TextButton.icon(
                    onPressed: () => state.syncNow(),
                    icon: const Icon(Icons.sync, size: 18),
                    label: Text(l.syncNow),
                  ),
                ),
              ),
          ],
        );
    }
  }
}

/// Runs the light speech-enhancement model instead of the full one.
///
/// **Off by default, and worded as a trade rather than as a quality knob.** On
/// a rider talking normally in still air the full model is the better of the
/// two; the light one is three times cheaper and takes 4 to 6 dB more out of
/// the speech. It is here for phones that cannot afford the full one, where
/// the alternative is the performance ladder taking the rest of the chain
/// apart instead.
class _SimpleModelTile extends StatelessWidget {
  const _SimpleModelTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.speed),
      title: Text(l.simpleModel),
      subtitle: Text(l.simpleModelBody),
      isThreeLine: true,
      value: state.simpleModel,
      onChanged: (v) => state.setSimpleModelEnabled(value: v),
    );
  }
}

class _ReverbTile extends StatelessWidget {
  const _ReverbTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    return SwitchListTile(
      secondary: const Icon(Icons.blur_on),
      title: Text(l.reverb),
      subtitle: Text(l.reverbBody),
      isThreeLine: true,
      value: state.reverb,
      onChanged: (v) => state.setReverbEnabled(value: v),
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
      title: Text(L.of(context).evenOutLoudness),
      subtitle: Text(L.of(context).evenOutLoudnessBody),
      isThreeLine: true,
      value: state.normaliseLevels,
      onChanged: (v) => state.setNormaliseLevels(value: v),
    );
  }
}

/// How much incoming audio is held back before it is played.
///
/// Given its own tile with the explanation attached rather than dropped into
/// the level sliders, because it is the one audio control where the right
/// answer depends on something outside the app — the network the rider is on —
/// and a number with no explanation is a number nobody will touch.
class _JitterBufferTile extends StatelessWidget {
  const _JitterBufferTile();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final state = AppStateScope.of(context);
    final (lo, hi, step) = AppState.jitterBounds;
    final value = state.jitterBufferMs.clamp(lo, hi).toDouble();
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const Icon(Icons.network_check, size: 20),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  l.jitterBuffer,
                  style: const TextStyle(fontSize: 15),
                ),
              ),
              Text(
                l.milliseconds(value.round()),
                style: const TextStyle(
                  fontSize: 12,
                  fontFeatures: [FontFeature.tabularFigures()],
                ),
              ),
            ],
          ),
          Slider(
            value: value,
            min: lo.toDouble(),
            max: hi.toDouble(),
            // One stop per whole packet. Voice arrives in 20 ms pieces and the
            // buffer counts in whole ones, so anything finer would be a
            // position the engine rounds away the moment it is let go of.
            divisions: (hi - lo) ~/ step,
            onChanged: (v) => state.setJitterBuffer(ms: v.round()),
          ),
          Text(
            l.jitterBufferBody,
            style: TextStyle(
              fontSize: 11,
              color: Theme.of(context).colorScheme.onSurfaceVariant,
            ),
          ),
          const SizedBox(height: 4),
        ],
      ),
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
        onPressed: () async {
          final messenger = ScaffoldMessenger.of(context);
          final error = await state.testOutput();
          if (error != null) {
            messenger.showSnackBar(SnackBar(content: Text(error)));
          }
        },
        child: Text(l.play),
      ),
    );
  }
}

/// Input gain and output volume, with the live meter alongside so the effect
/// of a change is immediately visible.
class _LevelsSection extends StatelessWidget {
  const _LevelsSection({this.audioError});

  /// Why the meter is not live, when it is not. The screen opens the devices
  /// itself, so this is a genuine failure rather than a state to wait out.
  final String? audioError;

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
          // This screen holds the devices open, so the meter is normally live
          // whether or not anything is connected. It being dead here means
          // the microphone could not be had at all — a bar that never moves
          // while somebody drags the gain slider under it is worse than
          // saying so. The sliders stay usable either way: they are
          // remembered settings, not live adjustments.
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: state.audioActive
                ? const LevelMeter(showScale: true)
                : Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const Icon(
                        Icons.mic_off,
                        size: 16,
                        color: StatusColors.failed,
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(
                          // The platform's own words when it gave any: they
                          // name the app holding the device, which is the
                          // only version anybody can act on.
                          audioError ?? l.micUnavailable,
                          style: TextStyle(
                            fontSize: 12,
                            color: Theme.of(
                              context,
                            ).colorScheme.onSurfaceVariant,
                          ),
                        ),
                      ),
                    ],
                  ),
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
            l.levelsHelp,
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
/// What the app is hearing from a Bluetooth remote.
///
/// Kept because it is the only way to tell two identical-looking failures
/// apart: a remote that sends nothing an app may see, and an app that is not
/// listening. iOS hands over only the transport buttons — volume and shutter
/// codes are consumed by the system before any third party sees them — so a
/// remote can be perfectly functional and still be invisible here, and the
/// only way to find out is to press one and watch.
class _ButtonDiagnostics extends StatelessWidget {
  const _ButtonDiagnostics();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final buttons = AppStateScope.of(context).buttons;
    if (buttons.captureState == null) return const SizedBox.shrink();

    final heard = buttons.lastMediaKey ?? buttons.lastKey;
    return Padding(
      padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
      child: Text(
        '${l.remoteListening}  ·  '
        '${heard == null ? l.remoteNothingYet : l.remoteLastButton(heard)}',
        style: TextStyle(
          fontSize: 11,
          color: Theme.of(context).colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

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
            subtitle: Text(b.action.label(l)),
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
                      DropdownMenuItem(value: a, child: Text(a.label(l))),
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

/// The proxy line, in the rider's language.
///
/// `ProxyConfig.description` builds the same sentence in English and is kept
/// for logs, where English is wanted. It was being shown in the interface too,
/// so a Russian settings screen read "Direct connection" under a switch whose
/// own label was translated.
String _proxyLine(L l, ProxyConfig c) {
  final proxy = c.proxy ?? '';
  return switch (c.source) {
    ProxySource.none => l.proxyDirect,
    ProxySource.system => l.proxySystemAt(proxy),
    ProxySource.environment => l.proxyEnvironmentAt(proxy),
    ProxySource.manual => l.proxyManualAt(proxy),
  };
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
            state.proxyEnabled
                ? _proxyLine(l, state.proxyConfig)
                : l.proxyOffDirect,
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
        return L.of(context).floatingAndroidBody;
      case FloatingKind.iosPictureInPicture:
        return L.of(context).floatingIosBody;
      case FloatingKind.none:
        return L.of(context).notAvailableHere;
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
        // A snackbar is gone in four seconds and this arrives after one. What
        // the system said has to stay on screen beside the switch that asked
        // it — and that includes success, because a window that opened and
        // cannot be seen looks exactly like one that never opened.
        if (state.overlayStatus case final status?)
          Padding(
            padding: const EdgeInsets.fromLTRB(72, 0, 20, 10),
            child: Text(
              status,
              style: TextStyle(
                fontSize: 11,
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
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
                  SnackBar(content: Text(L.of(context).fingerprintCopied)),
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

/// Which build this is, at the foot of the settings screen.
///
/// The first question asked of any bug report, and until now the answer was
/// not in the app at all — a tester had to go to TestFlight or the Play
/// listing to find it, and on a sideloaded build there was nowhere to look.
/// The build number is the part that matters: the marketing version stays
/// still for months while the number underneath it changes with every upload.
class _VersionFooter extends StatefulWidget {
  const _VersionFooter();

  @override
  State<_VersionFooter> createState() => _VersionFooterState();
}

class _VersionFooterState extends State<_VersionFooter> {
  String? _text;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    try {
      final info = await PackageInfo.fromPlatform();
      if (!mounted) return;
      setState(
        () => _text = '${info.appName} ${info.version} (${info.buildNumber})',
      );
    } catch (_) {
      // Nothing to show and nothing to say about it: an absent version line
      // is a smaller problem than an error where a version should be.
    }
  }

  @override
  Widget build(BuildContext context) {
    final text = _text;
    if (text == null) return const SizedBox(height: 16);

    return Center(
      // Selectable so it can be copied into a report rather than copied out
      // by eye, which is where build numbers get transposed.
      child: SelectableText(
        text,
        style: TextStyle(
          fontSize: 11,
          color: Theme.of(context).colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}
