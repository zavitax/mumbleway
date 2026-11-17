import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

/// Answers `mumbleway/secrets` the way a headless host does: no plugin.
///
/// `AppState._persist` now reaches [SecretStore], and an unmocked platform
/// channel inside `testWidgets` is never answered under its fake clock, so
/// every widget test that saved a setting hung until the ten-minute timeout
/// (`capture_idle_test.dart` among them). Throwing `MissingPluginException`
/// is what `SecretStore` already treats as "not available here", so the code
/// under test takes the same path it takes on a host with no keystore.
///
/// Tests that exercise the store itself install their own handler over this
/// one, as `secret_store_test.dart` does.
Future<void> testExecutable(FutureOr<void> Function() testMain) async {
  TestWidgetsFlutterBinding.ensureInitialized();
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(const MethodChannel('mumbleway/secrets'), (
        call,
      ) async {
        throw MissingPluginException();
      });
  await testMain();
}
