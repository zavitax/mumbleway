import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/server_proxy.dart';
import '../src/rust/api/mumbleway.dart';

/// What a rider chose in the proxy sheet.
///
/// The mode and the proxy travel together because three of the four choices
/// carry no address at all, and a null address with "custom" selected is a
/// half-answer nothing downstream could act on.
class ProxyChoice {
  const ProxyChoice({required this.mode, this.proxy}) : defaultMode = null;

  const ProxyChoice.forDefault({required this.defaultMode, this.proxy})
    : mode = null;

  /// What a *server* chose, or null when this came from the settings sheet.
  final ServerProxyMode? mode;

  /// What the *app* chose, or null when this came from a server's form.
  final DefaultProxyMode? defaultMode;

  /// The address, when one of the two modes above calls for it.
  final ServerProxy? proxy;
}

/// The one editor, used by the server form and by Settings.
///
/// **Two entry points, one sheet**, because the thing being described is the
/// same in both places and a rider who learns it once should not have to learn
/// it again. The difference is a single row: a server may defer to the app, and
/// the app may defer to the operating system.
Future<ProxyChoice?> showProxyEditor(
  BuildContext context, {
  /// Null on the settings sheet.
  ServerProxyMode? mode,
  DefaultProxyMode? defaultMode,
  ServerProxy? proxy,
}) {
  return showModalBottomSheet<ProxyChoice>(
    context: context,
    isScrollControlled: true,
    builder: (context) => _ProxySheet(
      mode: mode,
      defaultMode: defaultMode,
      proxy: proxy,
    ),
  );
}

class _ProxySheet extends StatefulWidget {
  const _ProxySheet({this.mode, this.defaultMode, this.proxy});

  final ServerProxyMode? mode;
  final DefaultProxyMode? defaultMode;
  final ServerProxy? proxy;

  @override
  State<_ProxySheet> createState() => _ProxySheetState();
}

/// The four rows, as one list whichever sheet this is.
enum _Row { direct, inherit, http, socks }

class _ProxySheetState extends State<_ProxySheet> {
  late _Row _row;
  late final TextEditingController _address;
  late final TextEditingController _user;
  late final TextEditingController _password;
  late bool _tunnelVoice;
  String? _error;

  bool get _forServer => widget.mode != null;

  @override
  void initState() {
    super.initState();
    _row = switch ((widget.mode, widget.defaultMode)) {
      (ServerProxyMode.direct, _) => _Row.direct,
      (ServerProxyMode.useDefault, _) => _Row.inherit,
      (_, DefaultProxyMode.direct) => _Row.direct,
      (_, DefaultProxyMode.system) => _Row.inherit,
      _ => widget.proxy?.scheme == ProxyScheme.socks5 ? _Row.socks : _Row.http,
    };
    _address = TextEditingController(
      text: widget.proxy == null
          ? ''
          : '${widget.proxy!.host}:${widget.proxy!.port}',
    );
    _user = TextEditingController(text: widget.proxy?.username ?? '');
    _password = TextEditingController(text: widget.proxy?.password ?? '');
    _tunnelVoice = widget.proxy?.tunnelVoice ?? false;
  }

  @override
  void dispose() {
    for (final c in [_address, _user, _password]) {
      c.dispose();
    }
    super.dispose();
  }

  bool get _needsAddress => _row == _Row.http || _row == _Row.socks;

  void _save() {
    final l = L.of(context);
    if (!_needsAddress) {
      Navigator.pop(
        context,
        _forServer
            ? ProxyChoice(
                mode: _row == _Row.direct
                    ? ServerProxyMode.direct
                    : ServerProxyMode.useDefault,
              )
            : ProxyChoice.forDefault(
                defaultMode: _row == _Row.direct
                    ? DefaultProxyMode.direct
                    : DefaultProxyMode.system,
              ),
      );
      return;
    }

    final parsed = parseProxy(
      _address.text,
      scheme: _row == _Row.socks ? ProxyScheme.socks5 : ProxyScheme.httpConnect,
      tunnelVoice: _tunnelVoice,
    );
    if (parsed == null) {
      // The address is the one field that can be wrong in a way the rider can
      // fix, so it says so here rather than failing at the next connection.
      setState(() => _error = l.proxyAddressNeeded);
      return;
    }
    final proxy = ServerProxy(
      scheme: parsed.scheme,
      host: parsed.host,
      port: parsed.port,
      username: _user.text.trim().isEmpty ? null : _user.text.trim(),
      password: _password.text.isEmpty ? null : _password.text,
      tunnelVoice: _tunnelVoice,
    );
    Navigator.pop(
      context,
      _forServer
          ? ProxyChoice(mode: ServerProxyMode.custom, proxy: proxy)
          : ProxyChoice.forDefault(
              defaultMode: DefaultProxyMode.custom,
              proxy: proxy,
            ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Padding(
      padding: EdgeInsets.only(
        left: 16,
        right: 16,
        top: 16,
        bottom: MediaQuery.of(context).viewInsets.bottom + 16,
      ),
      child: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              _forServer ? l.proxyForThisServer : l.proxyForServers,
              style: const TextStyle(fontSize: 18, fontWeight: FontWeight.w700),
            ),
            const SizedBox(height: 12),
            for (final row in _Row.values)
              ListTile(
                contentPadding: EdgeInsets.zero,
                dense: true,
                leading: Icon(
                  row == _row
                      ? Icons.radio_button_checked
                      : Icons.radio_button_unchecked,
                  color: row == _row
                      ? Theme.of(context).colorScheme.primary
                      : null,
                ),
                onTap: () => setState(() {
                  _row = row;
                  _error = null;
                }),
                title: Text(switch (row) {
                  _Row.direct => l.proxyNone,
                  _Row.inherit =>
                    _forServer ? l.proxyUseGlobal : l.proxyUseSystem,
                  _Row.http => l.proxySchemeHttp,
                  _Row.socks => l.proxySchemeSocks5,
                }),
              ),
            if (_needsAddress) ...[
              const SizedBox(height: 8),
              TextField(
                controller: _address,
                autocorrect: false,
                decoration: InputDecoration(
                  labelText: l.proxyHostPort,
                  hintText: l.proxyHostPortHint,
                  errorText: _error,
                ),
                onChanged: (_) {
                  if (_error != null) setState(() => _error = null);
                },
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _user,
                autocorrect: false,
                decoration: InputDecoration(labelText: l.proxyUsernameOptional),
              ),
              const SizedBox(height: 8),
              TextField(
                controller: _password,
                obscureText: true,
                decoration: InputDecoration(
                  labelText: l.proxyPasswordOptional,
                  helperText: l.proxyCredentialsStayHere,
                ),
              ),
              const SizedBox(height: 8),
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                value: _tunnelVoice,
                title: Text(l.proxyTunnelVoice),
                subtitle: Text(l.proxyTunnelVoiceHelp),
                onChanged: (v) => setState(() => _tunnelVoice = v),
              ),
            ],
            const SizedBox(height: 8),
            Row(
              mainAxisAlignment: MainAxisAlignment.end,
              children: [
                TextButton(
                  onPressed: () => Navigator.pop(context),
                  child: Text(l.cancel),
                ),
                const SizedBox(width: 8),
                FilledButton(onPressed: _save, child: Text(l.save)),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

/// What a choice reads as on the tile that opens the sheet.
String proxySummary(L l, ServerProxyMode mode, ServerProxy? proxy) =>
    switch (mode) {
      ServerProxyMode.direct => l.proxyNone,
      ServerProxyMode.useDefault => l.proxyUseGlobal,
      ServerProxyMode.custom =>
        proxy == null ? l.proxyNone : _address(l, proxy),
    };

String defaultProxySummary(L l, DefaultProxyMode mode, ServerProxy? proxy) =>
    switch (mode) {
      DefaultProxyMode.direct => l.proxyNone,
      DefaultProxyMode.system => l.proxyUseSystem,
      DefaultProxyMode.custom =>
        proxy == null ? l.proxyNone : _address(l, proxy),
    };

String _address(L l, ServerProxy proxy) {
  final kind = proxy.scheme == ProxyScheme.socks5
      ? l.proxySchemeSocks5
      : l.proxySchemeHttp;
  final voice = proxy.tunnelVoice ? ' · ${l.proxyVoiceThroughIt}' : '';
  return '$kind · ${proxy.host}:${proxy.port}$voice';
}
