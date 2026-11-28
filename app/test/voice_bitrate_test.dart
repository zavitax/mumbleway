import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/l10n/app_localizations.dart';
import 'package:mumbleway/state/app_state.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// The voice bitrate setting: "Auto" (stored as 0) or a fixed rate.
///
/// What the engine does with it is pinned in `core/src/audio/adaptive.rs`.
/// What is pinned here is that the interface never hands it a number the
/// slider could not have produced, and that the choice survives a restart.
void main() {
  setUp(() => SharedPreferences.setMockInitialValues({}));

  test('a fresh install keeps the Balanced rate, not Auto', () {
    final state = AppState();
    addTearDown(state.dispose);
    expect(state.voiceBitrateBps, AppState.defaultVoiceBitrateBps);
    expect(state.voiceBitrateBps, 24000);
    final (lo, hi, step) = AppState.bitrateBounds;
    expect(state.voiceBitrateBps, inInclusiveRange(lo, hi));
    expect((state.voiceBitrateBps - lo) % step, 0);
  });

  test('a fixed rate is snapped to the slider and remembered', () async {
    final state = AppState();
    addTearDown(state.dispose);
    final (lo, hi, step) = AppState.bitrateBounds;

    await state.setVoiceBitrateSetting(bps: 33000);
    expect(state.voiceBitrateBps, 32000);
    expect((state.voiceBitrateBps - lo) % step, 0);

    await state.setVoiceBitrateSetting(bps: 1000000);
    expect(state.voiceBitrateBps, hi);
    await state.setVoiceBitrateSetting(bps: 1);
    expect(state.voiceBitrateBps, lo);

    final prefs = await SharedPreferences.getInstance();
    expect(prefs.getInt('mumbleway.voiceBitrateBps'), lo);
  });

  test('going back to Auto is remembered too', () async {
    final state = AppState();
    addTearDown(state.dispose);
    await state.setVoiceBitrateSetting(bps: 48000);
    await state.setVoiceBitrateSetting(bps: 0);
    expect(state.voiceBitrateBps, 0);
    final prefs = await SharedPreferences.getInstance();
    expect(prefs.getInt('mumbleway.voiceBitrateBps'), 0);
  });

  test('the fixed default sits on the slider', () {
    final (lo, hi, step) = AppState.bitrateBounds;
    const d = AppState.defaultFixedBitrateBps;
    expect(d, inInclusiveRange(lo, hi));
    expect((d - lo) % step, 0);
  });

  test('the live rate reads in both languages', () async {
    for (final code in ['en', 'ru']) {
      final l = await L.delegate.load(Locale(code));
      expect(l.voiceBitrateAutoNow(40), contains('40'));
      expect(l.voiceBitrateBody, contains('32'));
    }
  });
}
