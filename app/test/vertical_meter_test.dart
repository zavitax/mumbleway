import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/widgets/voice_meter.dart';

/// Which way a meter standing on end fills.
///
/// **It filled downward from the top, and looked right until it moved.** A
/// `Stack`'s unpositioned child sits at the top left, so the growing part was
/// pinned there; and because the gradient keeps its own scale whatever is
/// shown, the slice on screen was the *green* bottom of it, arriving at the
/// top of the track. Quiet speech therefore painted a green cap at the top and
/// loud speech grew down towards the floor, which is every meter a rider has
/// ever used, upside down.
///
/// Geometry rather than pixels: the question is where the filled part sits in
/// the track, which `getRect` answers exactly and a screenshot answers only to
/// whoever is looking.
void main() {
  /// The filled part of the track.
  ///
  /// The rounded clip, not the gradient inside it: the gradient box keeps its
  /// full size on purpose — that is what makes a given loudness the same
  /// colour at any fill — so measuring *it* measures the whole track and would
  /// pass whichever way round the meter was.
  Finder fill() =>
      find.descendant(of: find.byType(VoiceMeter), matching: find.byType(ClipRRect));

  Widget host(Widget child) =>
      MaterialApp(home: Scaffold(body: Center(child: child)));

  testWidgets('a vertical meter grows up from the bottom', (t) async {
    await t.pumpWidget(
      host(
        const SizedBox(
          height: 20,
          child: VoiceMeter(vertical: true, width: 6, height: 20, levelDb: -20),
        ),
      ),
    );
    // Past the 100 ms tween, so the fill is where it is going to be.
    await t.pump(const Duration(milliseconds: 200));

    final track = t.getRect(find.byType(VoiceMeter));
    final filled = t.getRect(fill().first);

    // -20 dBFS on a floor of -50 is three fifths of the scale, so the fill is
    // somewhere in the middle: present, and not the whole track.
    expect(filled.bottom, closeTo(track.bottom, 0.5));
    expect(filled.top, greaterThan(track.top));
    expect(filled.height, lessThan(track.height));
  });

  testWidgets('and a loud one reaches the top', (t) async {
    await t.pumpWidget(
      host(
        const SizedBox(
          height: 20,
          child: VoiceMeter(vertical: true, width: 6, height: 20, levelDb: 0),
        ),
      ),
    );
    await t.pump(const Duration(milliseconds: 200));

    final track = t.getRect(find.byType(VoiceMeter));
    final filled = t.getRect(fill().first);
    expect(filled.bottom, closeTo(track.bottom, 0.5));
    expect(filled.top, closeTo(track.top, 0.5));
  });

  testWidgets('the horizontal one still grows from the left', (t) async {
    await t.pumpWidget(
      host(const SizedBox(width: 80, child: VoiceMeter(levelDb: -20))),
    );
    await t.pump(const Duration(milliseconds: 200));

    final track = t.getRect(find.byType(VoiceMeter));
    final filled = t.getRect(fill().first);
    expect(filled.left, closeTo(track.left, 0.5));
    expect(filled.right, lessThan(track.right));
  });
}
