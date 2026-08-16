import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../l10n/app_localizations.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../widgets/app_bar_title.dart';
import '../widgets/error_snack.dart';
import '../widgets/language_button.dart';
import 'import_screen.dart';
import '../widgets/qr_intake_button.dart';
import 'public_servers_screen.dart';

/// Form for adding a server, or editing one already saved.
///
/// One screen for both because the fields are identical and the difference is
/// entirely in what happens on save. Two screens would be two places to keep a
/// validation rule in step.
/// What a button looks like when the label needs the room more than the frame
/// does.
///
/// **Measured, because the default is more generous than it looks.**
/// `OutlinedButton` reserves 24 device pixels either side, and on a 360 dp
/// phone these two share a row with the QR glyph — so a 114 px button was
/// handing its label 66. That is narrower than the single word «Публичные»,
/// which is 127, so Russian had no choice but to break inside it.
///
/// Dropping the icons came first and was not enough on its own; this is the
/// other half. Eight pixels is still a visible inset and it is the difference
/// between a label that wraps between words and one that wraps through them.
final ButtonStyle _roomForWords = OutlinedButton.styleFrom(
  padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 12),
);

/// Below this many device pixels of available width, the two word buttons
/// stop sharing a row.
///
/// Derived rather than picked: the longest single word in either language is
/// «Публичные» at 127 px, "Импорт" needs 85, the glyph button takes 48 and the
/// two gaps 20 — which is 280 before either button's own inset. 340 leaves that
/// enough slack to survive a font that renders a little wider, and puts a
/// 360 dp phone on the two-row side, which is where it belongs.
const double _oneRowNeeds = 340;

class AddServerScreen extends StatefulWidget {
  const AddServerScreen({super.key, this.existing, this.prefill});

  /// The server being edited, or null when adding a new one.
  final SavedServer? existing;

  /// Details to start a *new* entry from, rather than one to edit.
  ///
  /// What arrives from a scanned code or a `mumble://` link followed from
  /// outside the app. Deliberately not [existing]: nothing has been saved yet,
  /// the form is still a draft, and the rider can change any of it — or back
  /// out — before anything is written. A link is a suggestion, not an
  /// instruction.
  final SavedServer? prefill;

  @override
  State<AddServerScreen> createState() => _AddServerScreenState();
}

class _AddServerScreenState extends State<AddServerScreen> {
  final _form = GlobalKey<FormState>();
  final _name = TextEditingController();
  final _host = TextEditingController();
  final _port = TextEditingController();
  final _user = TextEditingController();
  final _password = TextEditingController();
  bool _saving = false;

  bool get _editing => widget.existing != null;

  /// Where the form's contents came from.
  ///
  /// Kept because the entry carries things this form has no field for — the
  /// default channel, the pinned certificate — and they have to survive a save.
  /// It is not simply `widget.prefill`: a code scanned from the shortcuts row
  /// refills the form in place, and the channel from *that* invitation is the
  /// one that should be saved.
  SavedServer? _source;

  @override
  void initState() {
    super.initState();
    final source = widget.existing ?? widget.prefill;
    _source = source;
    if (source == null) {
      // 64738 is Mumble's registered port and the answer the engine gives.
      // Falling back to it rather than letting this throw: the engine may not
      // be up — startup failed, or this is a test — and a screen that cannot
      // draw at all is a worse failure than a port field the user can edit.
      var port = 64738;
      try {
        port = defaultPort();
      } catch (_) {}
      _port.text = port.toString();
      // A name to connect under, so the field is not simply blank and in the
      // way. Derived from the device rather than shared or invented, and only
      // a suggestion — it is an ordinary editable field.
      unawaited(_suggestUsername());
      return;
    }
    _name.text = source.name;
    _host.text = source.host;
    _port.text = source.port.toString();
    _user.text = source.username;
    _password.text = source.password ?? '';
  }

  /// Fills the username field with the device's own suggestion.
  ///
  /// Asynchronous because the answer comes from the platform, so the field is
  /// briefly empty. Only filled if it still is: a rider who types faster than
  /// a method channel answers should not have their name replaced.
  Future<void> _suggestUsername() async {
    // A suggestion, and only that. If the platform cannot answer — no engine,
    // an older host, a test — the field stays empty and the rider types a
    // name, which is what the field is for. Failing to guess is not a reason
    // to fail to draw the form.
    final String name;
    try {
      name = await AppStateScope.of(context).suggestedUsername();
    } catch (_) {
      return;
    }
    if (!mounted || _user.text.isNotEmpty) return;
    _user.text = name;
  }

  /// Refills the form from a scanned invitation.
  ///
  /// In place rather than on a new screen. This form is where the scan was
  /// started from, so a second copy of it stacked on top is one the rider has
  /// to save and then back out of — and saving popped them onto the empty
  /// first copy rather than onto their server list.
  void _fillFrom(SavedServer server) {
    setState(() {
      _source = server;
      _name.text = server.name;
      _host.text = server.host;
      _port.text = server.port.toString();
      _user.text = server.username;
      _password.text = server.password ?? '';
    });
  }

  @override
  void dispose() {
    for (final c in [_name, _host, _port, _user, _password]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(
        title: AppBarTitle(
          _editing ? l.editServer : l.addServer,
          showIcon: false,
        ),
        actions: const [LanguageButton()],
      ),
      body: Form(
        key: _form,
        child: ListView(
          padding: const EdgeInsets.all(16),
          children: [
            // Only useful when starting from nothing; while editing they would
            // navigate away from unsaved changes.
            if (!_editing) ...[
              _Shortcuts(onInvitation: _fillFrom),
              const SizedBox(height: 8),
            ],
            TextFormField(
              controller: _name,
              textInputAction: TextInputAction.next,
              decoration: InputDecoration(
                labelText: l.displayName,
                hintText: l.displayNameHint,
                prefixIcon: const Icon(Icons.label_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? l.displayNameMissing : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _host,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              keyboardType: TextInputType.url,
              decoration: InputDecoration(
                labelText: l.serverAddress,
                hintText: l.serverAddressHint,
                prefixIcon: const Icon(Icons.dns_outlined),
              ),
              validator: (v) => (v == null || v.trim().isEmpty)
                  ? l.serverAddressMissing
                  : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _port,
              textInputAction: TextInputAction.next,
              keyboardType: TextInputType.number,
              inputFormatters: [FilteringTextInputFormatter.digitsOnly],
              decoration: InputDecoration(
                labelText: l.port,
                prefixIcon: const Icon(Icons.numbers),
              ),
              validator: (v) {
                final p = int.tryParse(v ?? '');
                if (p == null || p < 1 || p > 65535) return l.portOutOfRange;
                return null;
              },
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _user,
              textInputAction: TextInputAction.next,
              autocorrect: false,
              decoration: InputDecoration(
                labelText: l.username,
                prefixIcon: const Icon(Icons.person_outline),
              ),
              validator: (v) =>
                  (v == null || v.trim().isEmpty) ? l.usernameMissing : null,
            ),
            const SizedBox(height: 12),
            TextFormField(
              controller: _password,
              obscureText: true,
              decoration: InputDecoration(
                labelText: l.passwordOptional,
                helperText: l.passwordHelp,
                prefixIcon: const Icon(Icons.lock_outline),
              ),
            ),
            const SizedBox(height: 24),
            FilledButton.icon(
              onPressed: _saving ? null : _submit,
              icon: _saving
                  ? const SizedBox(
                      width: 18,
                      height: 18,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.check),
              label: Text(_label(l)),
            ),
          ],
        ),
      ),
    );
  }

  String _label(L l) {
    if (_saving) return _editing ? l.savingChanges : l.addingServer;
    return _editing ? l.saveChanges : l.addServer;
  }

  Future<void> _submit() async {
    if (!_form.currentState!.validate()) return;
    setState(() => _saving = true);

    final state = AppStateScope.of(context);
    final existing = widget.existing;
    final draft = SavedServer(
      name: _name.text.trim(),
      host: _host.text.trim(),
      port: int.parse(_port.text),
      username: _user.text.trim(),
      password: _password.text.isEmpty ? null : _password.text,
      // Editing keeps the key and everything hanging off it; the pinned
      // certificate and default channel belong to the entry, not the form.
      localId: existing?.localId,
      certFingerprint: existing?.certFingerprint,
      // The channel has no field on this form, so it rides along from wherever
      // the entry came from: the saved one when editing, and the link's own
      // when a code or an invitation filled the form in.
      defaultChannel: existing?.defaultChannel ?? _source?.defaultChannel,
    );

    final error = existing == null
        ? await state.addNewServer(draft)
        : await state.updateServer(draft);

    if (!mounted) return;
    setState(() => _saving = false);

    if (error != null) {
      showError(ScaffoldMessenger.of(context), error);
      return;
    }
    Navigator.pop(context);
  }
}

/// Faster routes than typing an address by hand.
class _Shortcuts extends StatelessWidget {
  const _Shortcuts({required this.onInvitation});

  /// Called with the server a scanned code described, to fill the form this
  /// row sits on.
  final ValueChanged<SavedServer> onInvitation;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Card(
      margin: EdgeInsets.zero,
      child: Padding(
        padding: const EdgeInsets.all(14),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              l.quickerWays,
              style: const TextStyle(fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 10),
            // Sized to the tallest of the three rather than each to its own
            // content. Whether a label wraps depends on the width left over
            // and on the language — "Browse public" takes two lines on a
            // narrow phone and one on a wide one — and a row where one button
            // is visibly shorter than its neighbours reads as a mistake, not
            // as a smaller control.
            //
            // **Neither of the two word buttons carries an icon, and that is
            // the rule rather than an oversight.** An icon and its gap cost
            // about 34 of the ~130 device pixels one of these gets on a narrow
            // phone, and Russian needs every one of them: «Публичные серверы»
            // broke *inside* a word, which reads as a rendering fault rather
            // than as a long label. Where a label and a glyph compete for a
            // width this tight, the label wins — it is the thing being read,
            // and `Icons.public` and `Icons.download` add nothing a reader of
            // either language could not get from the words.
            //
            // The QR button beside them stays a glyph, because it has no label
            // to lose.
            // **One row where it fits, two where it does not.**
            //
            // Three controls, two of them carrying words, do not fit across a
            // 360 dp phone. Measured there: the long button had 66 device
            // pixels of text width against a first word of 127, so Russian
            // broke «Публичные» in half — which reads as a rendering fault
            // rather than as a long label.
            //
            // Dropping the icons and reclaiming the button padding took the
            // text width from 66 to 98, and weighting the split three-to-two
            // took it to 127.2 against a need of 126.9. That is a margin of
            // three tenths of a pixel, which is not a fix — the next font
            // metric change on somebody's phone takes it back.
            //
            // So below [`_oneRowNeeds`] the long label gets a row to itself and
            // the short one shares with the glyph. Both then have more room
            // than either language can use, in any font.
            LayoutBuilder(
              builder: (context, constraints) {
                final browse = OutlinedButton(
                  style: _roomForWords,
                  onPressed: () => Navigator.push(
                    context,
                    MaterialPageRoute(
                      builder: (_) => const PublicServersScreen(),
                    ),
                  ),
                  child: Text(l.browsePublic, textAlign: TextAlign.center),
                );
                final import = OutlinedButton(
                  style: _roomForWords,
                  onPressed: () => Navigator.push(
                    context,
                    MaterialPageRoute(builder: (_) => const ImportScreen()),
                  ),
                  child: Text(l.importLabel, textAlign: TextAlign.center),
                );
                final qr = QrIntakeButton(onInvitation: onInvitation);

                if (constraints.maxWidth < _oneRowNeeds) {
                  return Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      browse,
                      const SizedBox(height: 10),
                      IntrinsicHeight(
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            Expanded(child: import),
                            const SizedBox(width: 10),
                            qr,
                          ],
                        ),
                      ),
                    ],
                  );
                }

                // Sized to the tallest of the three rather than each to its own
                // content: a row where one button is visibly shorter than its
                // neighbours reads as a mistake, not as a smaller control.
                return IntrinsicHeight(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      // Not an equal share. "Browse public" is two words and
                      // "Import" is one, in both languages.
                      Expanded(flex: 3, child: browse),
                      const SizedBox(width: 10),
                      Expanded(flex: 2, child: import),
                      const SizedBox(width: 10),
                      // Not Expanded: the other two carry words and need the
                      // room, and a third equal share would squeeze all three.
                      // This one is a glyph and asks for only what a glyph
                      // needs.
                      qr,
                    ],
                  ),
                );
              },
            ),
          ],
        ),
      ),
    );
  }
}
