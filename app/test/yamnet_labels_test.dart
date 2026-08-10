import 'dart:io';

import 'package:archive/archive.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/services/background_classifier.dart';

/// The classifier reads one number out of 521, and picking the wrong one would
/// not fail — it would score a different class and the app would behave
/// plausibly and wrongly for the rest of its life. 132 is also the index
/// everyone quotes for `Music`, which is exactly the sort of number that gets
/// copied rather than checked.
///
/// A `.tflite` is a zip, and the MediaPipe build carries its own label list, so
/// the model can be asked directly.
void main() {
  test('class 132 really is Music in the model we ship', () {
    final file = File('assets/models/yamnet.tflite');
    expect(file.existsSync(), isTrue, reason: 'the model asset is missing');

    final zip = ZipDecoder().decodeBytes(file.readAsBytesSync());
    final list = zip.files.firstWhere(
      (f) => f.name.endsWith('.txt'),
      orElse: () => throw StateError('no label list inside the model'),
    );
    final labels = String.fromCharCodes(list.content as List<int>)
        .split('\n')
        .map((l) => l.trim())
        .where((l) => l.isNotEmpty)
        .toList();

    expect(labels.length, BackgroundClassifier.classes);
    expect(labels[BackgroundClassifier.musicIndex], 'Music');
  });

  test('the panel gets the three highest, highest first', () {
    // The panel shows what else the model was weighing, so the order is the
    // whole point: read as "Music is far ahead" or "it is a close-run thing".
    final scores = List<num>.filled(BackgroundClassifier.classes, 0.0);
    scores[10] = 0.4;
    scores[BackgroundClassifier.musicIndex] = 0.9;
    scores[300] = 0.65;
    scores[500] = 0.2;

    final top = BackgroundClassifier().highestForTest(scores);
    expect(top.length, 3);
    expect(top.map((c) => c.score).toList(), [0.9, 0.65, 0.4]);
    // No label list on a classifier that was never started, so it falls back
    // to the index — a number a reader can look up beats an empty row.
    expect(top.first.label, 'class ${BackgroundClassifier.musicIndex}');
  });
}
