import 'dart:io' show Platform;
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Suggests a name to connect under, taken from the device itself.
///
/// A username is not part of an invitation. It says who *this* rider is, and
/// copying one off a shared code gives two people on the same server the same
/// name — which Mumble resolves by refusing the second connection or by
/// appending a digit, neither of which is what anybody wanted. So the name is
/// derived here, on the device, and the invitation carries none.
///
/// What each platform can actually answer differs a great deal, and none of it
/// is guaranteed:
///
///   * Windows, Linux — the login name, from the environment.
///   * macOS — the account's full name, then its short name.
///   * Android — the device name the owner set, then the user profile name.
///   * iOS — the device name, which before iOS 16 was "Ilya's iPhone" and
///     since iOS 16 is just "iPhone" unless the app holds an entitlement that
///     a voice app has no business asking for.
///
/// Nothing here reads a Google or Apple account. Both platforms stopped
/// offering that to ordinary apps years ago — Android's `GET_ACCOUNTS` returns
/// nothing for accounts the app does not own, and iOS has never exposed the
/// Apple ID at all — and an app that asked would be asking for a contact
/// permission in order to guess a nickname.
///
/// When the platform offers nothing usable, two words and a dash. That is a
/// real answer rather than a placeholder: "amber-otter" is pronounceable over
/// a helmet intercom, distinct enough that two riders will not collide, and
/// obviously editable.
class DeviceIdentity {
  DeviceIdentity._();
  static final DeviceIdentity instance = DeviceIdentity._();

  static const _channel = MethodChannel('mumbleway/identity');

  /// Overrides the platform lookup, for tests.
  @visibleForTesting
  static Future<String?> Function()? platformOverride;

  /// Overrides the random source, so a test can assert an exact pair.
  @visibleForTesting
  static Random? randomOverride;

  /// A name for this device's rider.
  ///
  /// Never empty, and never throws: every failure ends at the word pair.
  Future<String> suggest() async {
    String? raw;
    try {
      raw = await _fromPlatform();
    } catch (_) {
      // Belt as well as braces. _fromPlatform guards the channel call it
      // expects to fail, but this is the only method anything else calls, and
      // a rider left staring at an empty username field because some platform
      // threw something unforeseen would have no idea what to type. There is
      // always a name to give them.
      raw = null;
    }
    return sanitize(raw) ?? randomName();
  }

  Future<String?> _fromPlatform() async {
    final override = platformOverride;
    if (override != null) return override();
    if (kIsWeb) return null;

    try {
      // The desktops answer without a method channel: a login name is an
      // environment variable on all three, and a channel would be three more
      // pieces of platform code to keep working for a value already in hand.
      if (Platform.isWindows) {
        return Platform.environment['USERNAME'];
      }
      if (Platform.isLinux) {
        return Platform.environment['USER'] ?? Platform.environment['LOGNAME'];
      }
      // macOS goes through the channel for NSFullUserName — "Ilya Melamed"
      // rather than "ilya" — and falls back to the environment if the channel
      // is not up, which is the case in a test harness.
      final answer = await _channel.invokeMethod<String>('suggestedName');
      if (answer != null && answer.trim().isNotEmpty) return answer;
      if (Platform.isMacOS) return Platform.environment['USER'];
      return null;
    } catch (_) {
      // No handler registered on this platform, or an older build of it. The
      // word pair is a perfectly good answer and needs nothing from anyone.
      return null;
    }
  }

  /// Turns whatever a platform said into something usable as a Mumble name,
  /// or null if there is nothing usable in it.
  ///
  /// Public and pure so the rules can be tested without a device attached.
  @visibleForTesting
  static String? sanitize(String? raw) {
    if (raw == null) return null;

    var s = raw.trim();
    if (s.isEmpty) return null;

    // "Ilya's iPhone" and "iPhone von Ilya" are the device's name, and the
    // part worth keeping is the person. English, German, French, Spanish and
    // Russian possessives cover most of what these fields actually contain.
    final possessive = RegExp(
      r"^(.+?)[’']s\s+\w+$|"
      r'^\w+\s+(?:von|de|di|van)\s+(.+)$',
      caseSensitive: false,
    );
    final owned = possessive.firstMatch(s);
    if (owned != null) {
      s = (owned.group(1) ?? owned.group(2) ?? s).trim();
    }

    // Mumble's default name policy is letters, digits, underscore, dot and
    // dash; anything else — spaces, apostrophes, and every non-Latin script —
    // is rejected by the server rather than by us. Spaces become dashes so a
    // full name survives as one; the rest simply goes.
    s = s
        .replaceAll(RegExp(r'\s+'), '-')
        .replaceAll(RegExp(r'[^A-Za-z0-9_.\-]'), '')
        .replaceAll(RegExp(r'-{2,}'), '-')
        .replaceAll(RegExp(r'^[-.]+|[-.]+$'), '');

    if (s.length < 2) return null;
    // Mumble's own limit. Truncated rather than rejected: a long full name is
    // still the right name, just a shorter one.
    if (s.length > 32) s = s.substring(0, 32).replaceAll(RegExp(r'[-.]+$'), '');

    // What a device says when it has nothing to say. Accepting these would
    // give every iPhone in the group the same name — the exact problem this
    // whole path exists to avoid — so they count as no answer at all.
    const useless = {
      'iphone', 'ipad', 'ipod', 'android', 'phone', 'tablet', 'mac',
      'macbook', 'macbookpro', 'macbookair', 'imac', 'localhost', 'user',
      'owner', 'admin', 'administrator', 'root', 'guest', 'default',
      'unknown', 'device', 'mobile', 'null', 'none', 'system',
    };
    if (useless.contains(s.toLowerCase())) return null;

    return s;
  }

  /// Two words and a dash, e.g. `amber-otter`.
  ///
  /// An adjective and a noun rather than two nouns: it reads as a name, and a
  /// list of colours and calm animals cannot combine into anything that would
  /// embarrass the rider it is handed to. 60 x 60 is 3600 pairs, which is far
  /// more than any group of riders will exhaust.
  @visibleForTesting
  static String randomName() {
    final r = randomOverride ?? Random.secure();
    return '${_adjectives[r.nextInt(_adjectives.length)]}'
        '-${_nouns[r.nextInt(_nouns.length)]}';
  }

  /// Colours, weather and temperament. Nothing about a person's body, and
  /// nothing that reads as an insult next to any noun below.
  static const List<String> _adjectives = [
    'amber', 'arctic', 'autumn', 'azure', 'brave', 'brisk', 'bronze', 'calm',
    'cedar', 'clever', 'copper', 'coral', 'crimson', 'dawn', 'dusty', 'eager',
    'early', 'ember', 'fleet', 'frosty', 'gentle', 'golden', 'granite',
    'hazel', 'humble', 'indigo', 'ivory', 'jade', 'keen', 'lively', 'lunar',
    'maple', 'marble', 'mellow', 'misty', 'noble', 'northern', 'olive',
    'onyx', 'polar', 'quiet', 'rapid', 'rugged', 'rustic', 'sable', 'sandy',
    'scarlet', 'silver', 'solar', 'steady', 'stellar', 'sunny', 'swift',
    'teal', 'tidal', 'umber', 'velvet', 'violet', 'wandering', 'winter',
  ];

  /// Animals and landscape. Chosen to be easy to say aloud over a noisy
  /// intercom, which is where these names get read out.
  static const List<String> _nouns = [
    'anchor', 'aspen', 'badger', 'beacon', 'birch', 'bison', 'boulder',
    'canyon', 'cedar', 'comet', 'condor', 'cove', 'crane', 'delta', 'dune',
    'eagle', 'elk', 'ember', 'falcon', 'fern', 'fjord', 'glacier', 'harbor',
    'harrier', 'heron', 'ibex', 'kestrel', 'lantern', 'lark', 'lynx', 'marten',
    'meadow', 'mesa', 'moose', 'osprey', 'otter', 'owl', 'pine', 'prairie',
    'quarry', 'raven', 'reef', 'ridge', 'river', 'sable', 'sparrow', 'spruce',
    'stag', 'summit', 'tarn', 'thistle', 'thrush', 'tundra', 'valley',
    'vulture', 'walrus', 'willow', 'wolf', 'wren', 'yarrow',
  ];
}
