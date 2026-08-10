import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/recording_player.dart';

/// Speech-only playback jumps from one transmitted stretch to the next, and a
/// rider judging whether a word start survived cannot judge it from a playhead
/// dropped onto the attack itself. So each stretch is played with
/// [kPlaybackLeadBlocks] of run-in in front of it.
///
/// **The pair is what needs testing, not the seek.** Backing the seek up while
/// leaving the end computed from "is this block transmitted" lands the playhead
/// on a silent block and returns a run of zero length, which the feed loop
/// breaks out of and never re-enters — silence, from a change that looks right
/// in isolation on either side.
void main() {
  /// A player with decisions but no file: the arithmetic is what is under test,
  /// and it needs neither an engine nor audio.
  RecordingPlayer withBlocks(List<int> flags) {
    final p = RecordingPlayer();
    p.loadForTest(Uint8List.fromList(flags), flags.length * kRecordingBlock);
    p.setSpeechOnly(true);
    return p;
  }

  /// `count` blocks with `sent` marked.
  List<int> mask(int count, Set<int> sent) =>
      List<int>.generate(count, (i) => sent.contains(i) ? 1 : 0);

  const block = kRecordingBlock;

  test('a stretch is reached with its run-in in front of it', () {
    // One transmitted stretch, blocks 50-59, in a log of 200.
    final p = withBlocks(mask(200, {for (var i = 50; i < 60; i++) i}));

    // Playback opens the lead ahead of block 50 rather than on it.
    final from = p.nextAudibleForTest(0);
    expect(from, (50 - kPlaybackLeadBlocks) * block);

    // And that position has somewhere to go: the run reaches past the end of
    // the transmitted blocks rather than stopping where it started.
    final end = p.audibleEndForTest(from);
    expect(end, greaterThan(from));
    expect(end, 60 * block);
  });

  test('a lead that runs off the front of the recording is clamped', () {
    final p = withBlocks(mask(200, {0, 1, 2}));
    expect(p.nextAudibleForTest(0), 0);
    expect(p.audibleEndForTest(0), 3 * block);
  });

  test('two stretches closer together than the lead do not split', () {
    // Blocks 40 and 48 sent, eight apart — inside the lead, so the gap is
    // run-in for the second and the two play as one. Splitting them would ramp
    // out and back in during what a listener hears as one word.
    final p = withBlocks(mask(200, {40, 48}));
    final from = p.nextAudibleForTest(0);
    expect(from, (40 - kPlaybackLeadBlocks) * block);
    expect(p.audibleEndForTest(from), 49 * block);
  });

  test('the playhead inside a run is left where it is', () {
    final p = withBlocks(mask(200, {for (var i = 50; i < 60; i++) i}));
    // Already playing the lead: seeking must not shunt it forward again, or
    // every feed would restart the same stretch.
    final inLead = (50 - kPlaybackLeadBlocks + 2) * block;
    expect(p.nextAudibleForTest(inLead), inLead);
    expect(p.nextAudibleForTest(55 * block), 55 * block);
  });

  test('the feed always has something to take', () {
    // The stall this pair exists to avoid: walk a whole recording the way
    // `_feed` does and assert every step makes progress.
    final p = withBlocks(mask(300, {
      for (var i = 30; i < 33; i++) i,
      for (var i = 120; i < 150; i++) i,
      299,
    }));
    const total = 300 * block;
    var cursor = 0;
    var steps = 0;
    while (cursor < total) {
      final from = p.nextAudibleForTest(cursor);
      if (from >= total) break;
      final end = p.audibleEndForTest(from);
      expect(end, greaterThan(from), reason: 'no progress at sample $from');
      cursor = end;
      expect(++steps, lessThan(50), reason: 'looped');
    }
  });

  test('the run-in does not apply when speech-only is off', () {
    final p = withBlocks(mask(200, {for (var i = 50; i < 60; i++) i}));
    p.setSpeechOnly(false);
    expect(p.nextAudibleForTest(0), 0);
    expect(p.audibleEndForTest(0), 200 * block);
  });
}
