import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';

/// The name of a suppression profile, in the reader's language.
///
/// Shared rather than repeated, because it is now said in two places that must
/// agree: the settings screen names the profile a rider chose, and the
/// diagnostics panel names the one Auto landed on. Two copies of this switch
/// would eventually disagree about one profile, and the disagreement would read
/// as the app being confused about which one is running.
String noiseProfileTitle(L l, NoiseSetting n) => switch (n) {
  NoiseSetting.off => l.noiseOff,
  NoiseSetting.light => l.noiseLight,
  NoiseSetting.standard => l.noiseStandard,
  NoiseSetting.helmet => l.noiseHelmet,
  NoiseSetting.auto => l.noiseAuto,
};
