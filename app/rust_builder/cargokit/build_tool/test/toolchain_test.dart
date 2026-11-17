// Run with `dart test -c source` on Dart 3.5+ (locked frontend_server_client 3.2.0 targets legacy JIT snapshot).
import 'dart:io';

import 'package:build_tool/src/builder.dart';
import 'package:build_tool/src/cargo.dart';
import 'package:build_tool/src/options.dart';
import 'package:build_tool/src/rustup.dart';
import 'package:build_tool/src/target.dart';
import 'package:build_tool/src/util.dart';
import 'package:path/path.dart' as path;
import 'package:test/test.dart';
import 'package:yaml/yaml.dart';

void main() {
  group('findRustToolchainChannel', () {
    test('walks upward from manifestDir to locate rust-toolchain.toml', () {
      final root = Directory.systemTemp.createTempSync('cargokit_tc_');
      addTearDown(() => root.deleteSync(recursive: true));

      File(path.join(root.path, 'rust-toolchain.toml')).writeAsStringSync('''
[toolchain]
channel = "1.100.0"
components = ["clippy", "rustfmt"]
profile = "minimal"
''');
      final manifestDir = Directory(path.join(root.path, 'app', 'rust'))
        ..createSync(recursive: true);

      expect(findRustToolchainChannel(manifestDir.path), equals('1.100.0'));
    });

    test('prefers nearest rust-toolchain.toml over parent', () {
      final root = Directory.systemTemp.createTempSync('cargokit_tc_nearest_');
      addTearDown(() => root.deleteSync(recursive: true));

      File(path.join(root.path, 'rust-toolchain.toml')).writeAsStringSync('''
[toolchain]
channel = "1.99.0"
''');
      final appDir = Directory(path.join(root.path, 'app'))..createSync();
      File(path.join(appDir.path, 'rust-toolchain.toml')).writeAsStringSync('''
[toolchain]
channel = "1.100.0"
''');
      final manifestDir = Directory(path.join(appDir.path, 'rust'))
        ..createSync();

      expect(findRustToolchainChannel(manifestDir.path), equals('1.100.0'));
    });

    test('returns null when no rust-toolchain.toml exists', () {
      final root = Directory.systemTemp.createTempSync('cargokit_tc_none_');
      addTearDown(() => root.deleteSync(recursive: true));

      final manifestDir = Directory(path.join(root.path, 'app', 'rust'))
        ..createSync(recursive: true);

      expect(findRustToolchainChannel(manifestDir.path), isNull);
    });
  });

  group('RustBuilder toolchain resolution', () {
    test('uses rust-toolchain.toml when cargokit options omit toolchain',
        () async {
      final root = Directory.systemTemp.createTempSync('cargokit_builder_');
      addTearDown(() => root.deleteSync(recursive: true));

      File(path.join(root.path, 'rust-toolchain.toml')).writeAsStringSync('''
[toolchain]
channel = "1.100.0"
''');
      final manifestDir = Directory(path.join(root.path, 'app', 'rust'))
        ..createSync(recursive: true);
      final targetTempDir = path.join(root.path, 'target');

      final crateOptions = CargokitCrateOptions.parse(loadYamlNode('''
cargo:
  release:
    extra_flags: ["--locked"]
'''));

      final env = BuildEnvironment(
        configuration: BuildConfiguration.release,
        crateOptions: crateOptions,
        targetTempDir: targetTempDir,
        manifestDir: manifestDir.path,
        crateInfo: CrateInfo(packageName: 'rust_lib_mumbleway'),
        isAndroid: false,
      );

      final builder = RustBuilder(
        target: Target.forRustTriple('x86_64-unknown-linux-gnu')!,
        environment: env,
      );

      expect(builder.toolchain, equals('1.100.0'));

      final calls = <TestRunCommandArgs>[];
      testRunCommandOverride = (args) {
        calls.add(args);
        return TestRunCommandResult();
      };
      addTearDown(() => testRunCommandOverride = null);

      await builder.build();

      expect(calls, hasLength(1));
      expect(calls.single.executable, equals('rustup'));
      expect(calls.single.arguments.take(3).toList(),
          equals(['run', '1.100.0', 'cargo']));
      expect(calls.single.workingDirectory, equals(manifestDir.path));
    });

    test('respects explicit toolchain in cargokit.yaml over rust-toolchain.toml',
        () {
      final root = Directory.systemTemp.createTempSync('cargokit_override_');
      addTearDown(() => root.deleteSync(recursive: true));

      File(path.join(root.path, 'rust-toolchain.toml')).writeAsStringSync('''
[toolchain]
channel = "1.100.0"
''');
      final manifestDir = Directory(path.join(root.path, 'app', 'rust'))
        ..createSync(recursive: true);

      final crateOptions = CargokitCrateOptions.parse(loadYamlNode('''
cargo:
  release:
    toolchain: nightly
'''));

      final env = BuildEnvironment(
        configuration: BuildConfiguration.release,
        crateOptions: crateOptions,
        targetTempDir: path.join(root.path, 'target'),
        manifestDir: manifestDir.path,
        crateInfo: CrateInfo(packageName: 'rust_lib_mumbleway'),
        isAndroid: false,
      );

      final builder = RustBuilder(
        target: Target.forRustTriple('x86_64-unknown-linux-gnu')!,
        environment: env,
      );

      expect(builder.toolchain, equals('nightly'));
    });
  });

  group('Rustup', () {
    test('detects versioned toolchains and targets from rustup output', () {
      final calls = <TestRunCommandArgs>[];
      testRunCommandOverride = (args) {
        calls.add(args);
        if (args.arguments.length >= 2 &&
            args.arguments[0] == 'toolchain' &&
            args.arguments[1] == 'list') {
          return TestRunCommandResult(
            stdout: '1.100.0-x86_64-pc-windows-msvc (active, default)\r\n'
                'stable-x86_64-pc-windows-msvc\r\n'
                'custom-local\r\n',
          );
        }
        if (args.arguments.length >= 4 &&
            args.arguments[0] == 'target' &&
            args.arguments[1] == 'list') {
          return TestRunCommandResult(
            stdout: 'x86_64-pc-windows-msvc\r\naarch64-linux-android\r\n',
          );
        }
        return TestRunCommandResult();
      };
      addTearDown(() => testRunCommandOverride = null);

      final rustup = Rustup();

      expect(
        rustup.installedTargets('1.100.0'),
        equals(['x86_64-pc-windows-msvc', 'aarch64-linux-android']),
      );
      expect(
        calls.where((c) =>
            c.arguments.contains('custom-local') ||
            c.arguments.contains('install')),
        isEmpty,
      );
    });
  });
}
