import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/src/rust/api/mumbleway.dart' show UiAcousticStats;
import 'package:mumbleway/widgets/tap_stages.dart';

/// The lamp row is only honest if a dead microphone darkens it.
///
/// **The mask is held on the detector's own frame clock**, which advances only
/// while audio arrives. So a stream that stops does not clear the lamps — it
/// *freezes* them at whatever last passed, and the row would go on describing
/// a tap from a minute ago. That is the exact failure this feature exists to
/// expose, so it must not be the failure the feature has.
///
/// The frame count is the only thing that says whether anything is arriving,
/// and comparing it is a job for whoever polls. This pins that rule.
void main() {
  UiAcousticStats stats({required int frames, required int mask}) =>
      UiAcousticStats(
        enabled: true,
        frames: BigInt.from(frames),
        candidates: BigInt.zero,
        discardedLong: BigInt.zero,
        banked: BigInt.zero,
        gestures: BigInt.zero,
        levelDb: -40,
        floorDb: -60,
        meterDb: -40,
        peakDb: 0,
        impulseAttackDb: 0,
        impulseOverDb: 0,
        impulseDecayDb: 0,
        discardedRinging: BigInt.zero,
        speechBlocks: BigInt.zero,
        run: 0,
        stageMask: mask,
        recentAttackDb: 0,
        recentOverDb: 0,
      );

  test('the first poll draws nothing, because nothing is known yet', () {
    final reader = TapStageReader();
    expect(reader.read(stats(frames: 1000, mask: 0x1ff)), 0);
  });

  test('frames moving passes the mask through', () {
    final reader = TapStageReader();
    reader.read(stats(frames: 1000, mask: 0));
    expect(reader.read(stats(frames: 1100, mask: 0x1ff)), 0x1ff);
    expect(reader.read(stats(frames: 1200, mask: 0x007)), 0x007);
  });

  test('a stalled frame count darkens the row, whatever the mask says', () {
    final reader = TapStageReader();
    reader.read(stats(frames: 1000, mask: 0));
    expect(reader.read(stats(frames: 1100, mask: 0x1ff)), 0x1ff);

    // The microphone died here. The engine's mask is frozen at the last thing
    // that passed, and reporting it would be a row that lies.
    expect(
      reader.read(stats(frames: 1100, mask: 0x1ff)),
      0,
      reason: 'a frozen mask was drawn as live',
    );
    expect(reader.read(stats(frames: 1100, mask: 0x1ff)), 0);
  });

  test('and it comes back when frames do', () {
    final reader = TapStageReader();
    reader.read(stats(frames: 1000, mask: 0x1ff));
    reader.read(stats(frames: 1000, mask: 0x1ff));
    expect(reader.read(stats(frames: 1001, mask: 0x1ff)), 0x1ff);
  });
}
