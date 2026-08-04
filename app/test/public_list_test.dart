import 'package:flutter_test/flutter_test.dart';
import 'package:mumbleway/state/app_state.dart';

/// The public directory is XML, so every name arrives escaped. Getting this
/// wrong is invisible until a server with an ampersand in its name shows up in
/// the list reading `Dordogne &amp; Suisse`, which is how it was found.
void main() {
  group('parsePublicList', () {
    test('decodes named entities in server names', () {
      final servers = AppState.parsePublicList(
        '<server name="Dordogne &amp; Suisse" ip="1.2.3.4" port="64738" '
        'country="France" />',
      );

      expect(servers, hasLength(1));
      expect(servers.single.name, 'Dordogne & Suisse');
    });

    test('decodes every entity XML allows in an attribute', () {
      final servers = AppState.parsePublicList(
        '<server name="&lt;a&gt; &quot;b&quot; &apos;c&apos; &amp; d" '
        'ip="1.2.3.4" port="64738" />',
      );

      expect(servers.single.name, '''<a> "b" 'c' & d''');
    });

    test('decodes decimal and hexadecimal character references', () {
      final servers = AppState.parsePublicList(
        '<server name="caf&#233; &#x41;&#x42;" ip="1.2.3.4" port="64738" />',
      );

      expect(servers.single.name, 'café AB');
    });

    // The reason the implementation is a single pass. Decoding `&amp;` first
    // and the rest afterwards yields `<`, which is a different name than the
    // one the server published.
    test('does not decode its own output', () {
      final servers = AppState.parsePublicList(
        '<server name="&amp;lt; &amp;amp;" ip="1.2.3.4" port="64738" />',
      );

      expect(servers.single.name, '&lt; &amp;');
    });

    test('leaves malformed and unrepresentable references alone', () {
      final servers = AppState.parsePublicList(
        // A bare ampersand, an unknown entity, a lone surrogate and a value
        // past the end of Unicode. None of them should throw, and none should
        // cost the rest of the listing.
        '<server name="a &amp b &unknown; &#xD800; &#9999999;" '
        'ip="1.2.3.4" port="64738" />'
        '<server name="Intact" ip="5.6.7.8" port="64738" />',
      );

      expect(servers, hasLength(2));
      expect(servers.first.name, 'a &amp b &unknown; &#xD800; &#9999999;');
      expect(servers.last.name, 'Intact');
    });

    test('decodes the country field too', () {
      final servers = AppState.parsePublicList(
        '<server name="X" ip="1.2.3.4" port="64738" '
        'country="Cote d&apos;Ivoire" />',
      );

      expect(servers.single.country, "Cote d'Ivoire");
    });

    test('leaves an unescaped name untouched', () {
      final servers = AppState.parsePublicList(
        '<server name="Plain Name" ip="1.2.3.4" port="64738" />',
      );

      expect(servers.single.name, 'Plain Name');
    });
  });
}
