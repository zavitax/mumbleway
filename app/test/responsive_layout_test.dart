import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/widgets/server_detail_pane.dart';

/// Sizes taken from real devices, in logical pixels, so a regression here says
/// which device it broke rather than which number changed.
///
/// The point of the whole pass: a phone turned sideways should get the layout
/// an iPad gets, because at that width it fits.
void main() {
  group('two-pane breakpoint', () {
    test('a phone in portrait stays a single column', () {
      // iPhone 15, Pixel 7, iPhone SE: none of these has room beside a card.
      for (final width in [393.0, 412.0, 375.0]) {
        expect(
          width >= kWideLayoutBreakpoint,
          isFalse,
          reason: 'a $width-point portrait phone was given two panes',
        );
      }
    });

    test('a phone in landscape gets the tablet layout', () {
      // iPhone 15 (852), iPhone 15 Pro Max (932), Pixel 7 (892), iPhone 13
      // mini (812). The old 900 breakpoint let only one of these through.
      for (final width in [852.0, 932.0, 892.0, 812.0]) {
        expect(
          width >= kWideLayoutBreakpoint,
          isTrue,
          reason: 'a $width-point landscape phone was left in one column',
        );
      }
    });

    test('a small phone in landscape stays a single column', () {
      // 667 is an iPhone SE on its side. Two panes here would leave the detail
      // side narrower than the cards beside it, which is worse than one column.
      expect(667.0 >= kWideLayoutBreakpoint, isFalse);
    });

    test('every tablet size is comfortably above it', () {
      // iPad mini portrait, iPad Pro 11 portrait, iPad Pro 12.9 landscape.
      for (final width in [744.0, 834.0, 1366.0]) {
        expect(width >= kWideLayoutBreakpoint, isTrue);
      }
    });
  });

  group('master pane width', () {
    test('leaves the detail pane the majority on every screen', () {
      for (final total in [720.0, 812.0, 852.0, 932.0, 1024.0, 1366.0, 1920.0]) {
        final master = masterPaneWidth(total);
        expect(
          master,
          lessThan(total / 2),
          reason: 'the list took half or more of a $total-point screen',
        );
      }
    });

    test('never narrows past what a server card needs', () {
      expect(masterPaneWidth(kWideLayoutBreakpoint), greaterThanOrEqualTo(320));
      // Even asked for something absurd, the floor holds.
      expect(masterPaneWidth(100), 320);
    });

    test('stops widening once a list of cards has all it can use', () {
      // A desktop window gains nothing from a wider column of cards, so the
      // extra space goes to the pane that can use it.
      expect(masterPaneWidth(1920), 400);
      expect(masterPaneWidth(3840), 400);
    });

    test('a landscape phone splits nearer even than a desktop does', () {
      // The proportional middle of the range is what keeps a 852-point phone
      // from handing the roster a slot narrower than the cards beside it.
      expect(masterPaneWidth(852), closeTo(357.8, 0.1));
      expect(masterPaneWidth(1024), 400);
    });
  });
}
