import 'dart:io';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';

/// The macOS classifier dylib has to be called what it says it is called.
///
/// Build 109 died in dyld before reaching `main`, on every Mac:
///
/// ```text
/// Library not loaded: @rpath/libtensorflowlite_c.dylib
/// tried: '/Applications/mumbleway.app/Contents/Frameworks/
///        libtensorflowlite_c.dylib' (no such file)
/// ```
///
/// Three places name this file and all three have to agree, because
/// `vendored_libraries` **links** the dylib as well as copying it:
///
/// * its own `LC_ID_DYLIB`, which the linker copies into the app binary as the
///   name to look for at launch;
/// * the podspec, which decides what the file is called in `Frameworks`;
/// * `bindings.dart`, which opens it by path afterwards.
///
/// It shipped as `libtensorflowlite_c-mac.dylib` with an install name of
/// `@rpath/libtensorflowlite_c.dylib`, so dyld looked for a file that was not
/// there. Nothing in a Windows or Android build can see this, and the macOS CI
/// job compiles and signs without ever loading the result — so the first thing
/// that noticed was a crash report from a real Mac.
void main() {
  const dir = 'third_party/tflite_flutter/macos';

  /// The `LC_ID_DYLIB` of every architecture in a Mach-O, fat or thin.
  List<String> installNames(File file) {
    final bytes = file.readAsBytesSync();
    final data = ByteData.sublistView(bytes);

    // A universal binary's header is big-endian, and its slices are ordinary
    // Mach-O images at the offsets it lists.
    final slices = <int>[];
    final leading = data.getUint32(0, Endian.big);
    if (leading == 0xcafebabe || leading == 0xcafebabf) {
      final count = data.getUint32(4, Endian.big);
      for (var i = 0; i < count; i++) {
        slices.add(data.getUint32(8 + i * 20 + 8, Endian.big));
      }
    } else {
      slices.add(0);
    }

    const lcIdDylib = 0xd;
    final names = <String>[];
    for (final start in slices) {
      // 64-bit little-endian Mach-O only; nothing here ships 32-bit.
      expect(data.getUint32(start, Endian.little), 0xfeedfacf,
          reason: 'slice at $start is not a 64-bit Mach-O');
      final commands = data.getUint32(start + 16, Endian.little);
      var at = start + 32; // past mach_header_64
      for (var i = 0; i < commands; i++) {
        final cmd = data.getUint32(at, Endian.little);
        final size = data.getUint32(at + 4, Endian.little);
        if (cmd == lcIdDylib) {
          final offset = data.getUint32(at + 8, Endian.little);
          final raw = bytes.sublist(at + offset, at + size);
          final end = raw.indexOf(0);
          names.add(String.fromCharCodes(end == -1 ? raw : raw.sublist(0, end)));
        }
        at += size;
      }
    }
    return names;
  }

  test('the dylib is named what its install name says', () {
    final podspec = File('$dir/tflite_flutter.podspec').readAsStringSync();
    final vendored =
        RegExp(r"vendored_libraries\s*=\s*'([^']+)'").firstMatch(podspec);
    expect(vendored, isNotNull, reason: 'the podspec must vendor the dylib');
    final filename = vendored!.group(1)!;

    final file = File('$dir/$filename');
    expect(file.existsSync(), isTrue, reason: '$dir/$filename is missing');

    final names = installNames(file);
    expect(names, isNotEmpty, reason: 'no LC_ID_DYLIB in $filename');
    for (final name in names) {
      // `@rpath/libtensorflowlite_c.dylib` -> `libtensorflowlite_c.dylib`
      expect(name.split('/').last, filename,
          reason: 'the dylib calls itself "$name" and CocoaPods will copy it '
              'into Frameworks as "$filename". dyld looks for the former and '
              'finds the latter, and the app dies before main.');
    }
  });

  test('bindings.dart opens the file the podspec ships', () {
    final podspec = File('$dir/tflite_flutter.podspec').readAsStringSync();
    final filename = RegExp(r"vendored_libraries\s*=\s*'([^']+)'")
        .firstMatch(podspec)!
        .group(1)!;

    final bindings =
        File('third_party/tflite_flutter/lib/src/bindings/bindings.dart')
            .readAsStringSync();
    // The macOS branch only: Linux and Windows name their own blobs, and those
    // are loaded by path from somewhere else entirely.
    final macos = RegExp(r'isMacOS[\s\S]*?DynamicLibrary\.open\(\s*([^;]+);')
        .firstMatch(bindings);
    expect(macos, isNotNull, reason: 'no macOS branch in bindings.dart');
    expect(macos!.group(1), contains('Frameworks/$filename'),
        reason: 'bindings.dart opens a different name than the podspec ships');
  });

  test('it is still universal, so Apple silicon and Intel both load it', () {
    // Shipping one architecture would not crash here; it would crash on
    // somebody else's Mac.
    final file = File('$dir/libtensorflowlite_c.dylib');
    final data = ByteData.sublistView(file.readAsBytesSync());
    expect(data.getUint32(0, Endian.big), anyOf(0xcafebabe, 0xcafebabf),
        reason: 'not a universal binary');
    expect(data.getUint32(4, Endian.big), greaterThanOrEqualTo(2),
        reason: 'a universal binary with fewer than two slices');
  });
}
