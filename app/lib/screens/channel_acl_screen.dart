import 'package:flutter/material.dart';

import '../l10n/app_localizations.dart';
import '../services/channel_permissions.dart';
import '../src/rust/api/mumbleway.dart';
import '../state/app_state.dart';
import '../theme.dart';
import '../widgets/error_snack.dart';
import '../widgets/watch.dart';

/// Who may do what in one channel.
///
/// **The whole list is written back, never one rule.** The protocol has no way
/// to change a single entry, so this screen edits a copy and sends all of it —
/// which is why it reads the list again afterwards and shows what the server
/// kept rather than what was sent. A rule lost on the way through would be a
/// permission revoked by accident.
///
/// Rules inherited from a parent channel are shown and cannot be edited: they
/// belong to the channel that defines them, and a server drops them from
/// anything written here. Showing them anyway matters — half the reason a
/// permission behaves unexpectedly is a rule set somewhere above.
class ChannelAclScreen extends StatefulWidget {
  const ChannelAclScreen({
    super.key,
    required this.serverId,
    required this.channel,
  });

  final String serverId;
  final UiChannel channel;

  @override
  State<ChannelAclScreen> createState() => _ChannelAclScreenState();
}

class _ChannelAclScreenState extends State<ChannelAclScreen> {
  /// The rider's edits, or null when they have made none.
  ///
  /// **Null means "whatever the server last said"** rather than "not loaded
  /// yet", which is what keeps this screen from writing state during a build:
  /// an earlier version adopted the server's copy inside `build` and threw
  /// `setState() called during build` the moment the answer arrived.
  UiChannelAcl? _draft;
  bool _asked = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_asked) return;
    _asked = true;
    AppStateScope.of(context).loadAcl(widget.serverId, widget.channel.id);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(
        title: Text(
          l.aclTitle(
            widget.channel.name.isEmpty ? '(root)' : widget.channel.name,
          ),
        ),
      ),
      body: Watch<UiChannelAcl?>(
        (state) => state.runtimeFor(widget.serverId).acls[widget.channel.id],
        (context, state) {
          // The rider's edits if they have made any, otherwise the server's
          // own copy. No state is written here; see the field.
          final draft =
              _draft ??
              state.runtimeFor(widget.serverId).acls[widget.channel.id];
          if (draft == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              SwitchListTile(
                contentPadding: EdgeInsets.zero,
                value: draft.inheritAcls,
                title: Text(l.aclInherit),
                onChanged: (v) => setState(
                  () => _draft = _withInherit(draft, v),
                ),
              ),
              const Divider(height: 24),
              Text(l.aclRules, style: _heading(context)),
              Text(l.aclTapHint, style: _quiet(context)),
              const SizedBox(height: 8),
              for (final (i, rule) in draft.rules.indexed)
                _RuleCard(
                  rule: rule,
                  names: state.runtimeFor(widget.serverId).userNames,
                  onChanged: rule.inherited
                      ? null
                      : (next) => setState(() {
                          final rules = [...draft.rules];
                          rules[i] = next;
                          _draft = _withRules(draft, rules);
                        }),
                  onRemove: rule.inherited
                      ? null
                      : () => setState(() {
                          final rules = [...draft.rules]..removeAt(i);
                          _draft = _withRules(draft, rules);
                        }),
                ),
              const SizedBox(height: 8),
              OutlinedButton.icon(
                icon: const Icon(Icons.add),
                label: Text(l.aclAddRule),
                onPressed: () => setState(() {
                  _draft = _withRules(draft, [
                    ...draft.rules,
                    const UiAclRule(
                      applyHere: true,
                      applySubs: true,
                      inherited: false,
                      userId: null,
                      group: 'all',
                      grant: 0,
                      deny: 0,
                    ),
                  ]);
                }),
              ),
              const Divider(height: 24),
              Text(l.aclGroups, style: _heading(context)),
              for (final g in draft.groups)
                ListTile(
                  dense: true,
                  contentPadding: EdgeInsets.zero,
                  title: Text(g.name),
                  subtitle: Text(
                    [
                      l.aclGroupMembers(
                        g.add.length + g.inheritedMembers.length,
                      ),
                      if (g.inherited) l.aclInherited,
                    ].join(' · '),
                    style: _quiet(context),
                  ),
                ),
              const SizedBox(height: 24),
              FilledButton(
                onPressed: () async {
                  final messenger = ScaffoldMessenger.of(context);
                  final error = await state.saveAcl(widget.serverId, draft);
                  if (error != null) {
                    showError(messenger, error);
                  } else {
                    messenger.showSnackBar(
                      SnackBar(content: Text(l.aclSaved)),
                    );
                    // Let the server's answer become the draft again, so what
                    // is on screen is what it kept.
                    setState(() => _draft = null);
                  }
                },
                child: Text(l.aclSave),
              ),
            ],
          );
        },
      ),
    );
  }

  TextStyle _heading(BuildContext context) =>
      const TextStyle(fontWeight: FontWeight.w700, fontSize: 14);

  TextStyle _quiet(BuildContext context) => TextStyle(
    fontSize: 12,
    color: Theme.of(context).colorScheme.onSurfaceVariant,
  );

  UiChannelAcl _withInherit(UiChannelAcl acl, bool inherit) => UiChannelAcl(
    channelId: acl.channelId,
    inheritAcls: inherit,
    groups: acl.groups,
    rules: acl.rules,
  );

  UiChannelAcl _withRules(UiChannelAcl acl, List<UiAclRule> rules) =>
      UiChannelAcl(
        channelId: acl.channelId,
        inheritAcls: acl.inheritAcls,
        groups: acl.groups,
        rules: rules,
      );
}

/// One rule: who it is about, where it applies, and what it says.
class _RuleCard extends StatelessWidget {
  const _RuleCard({
    required this.rule,
    required this.names,
    required this.onChanged,
    required this.onRemove,
  });

  final UiAclRule rule;
  final Map<int, String> names;
  final void Function(UiAclRule)? onChanged;
  final VoidCallback? onRemove;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final who = switch ((rule.group, rule.userId)) {
      (final String g, _) => l.aclGroupNamed(_groupLabel(l, g)),
      (_, final int id) => l.aclUserNamed(names[id] ?? '#$id'),
      _ => l.aclFor,
    };

    return Card(
      margin: const EdgeInsets.symmetric(vertical: 4),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    who,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
                if (rule.inherited)
                  Text(l.aclInherited, style: TextStyle(
                    fontSize: 11,
                    color: scheme.onSurfaceVariant,
                  ))
                else if (onRemove != null)
                  IconButton(
                    tooltip: l.aclRemoveRule,
                    icon: const Icon(Icons.delete_outline, size: 18),
                    onPressed: onRemove,
                  ),
              ],
            ),
            Text(
              describeRule(l, rule.grant, rule.deny),
              style: TextStyle(fontSize: 12, color: scheme.onSurfaceVariant),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final p in ChannelPermission.values)
                  _PermissionChip(
                    label: p.label(l),
                    stand: standOf(rule.grant, rule.deny, p),
                    onTap: onChanged == null
                        ? null
                        : () {
                            final next = nextStand(
                              standOf(rule.grant, rule.deny, p),
                            );
                            final (g, d) = applyStand(
                              rule.grant,
                              rule.deny,
                              p,
                              next,
                            );
                            onChanged!(
                              UiAclRule(
                                applyHere: rule.applyHere,
                                applySubs: rule.applySubs,
                                inherited: rule.inherited,
                                userId: rule.userId,
                                group: rule.group,
                                grant: g,
                                deny: d,
                              ),
                            );
                          },
                  ),
              ],
            ),
            if (onChanged != null)
              Row(
                children: [
                  _Flag(
                    label: l.aclApplyHere,
                    value: rule.applyHere,
                    onChanged: (v) => onChanged!(
                      UiAclRule(
                        applyHere: v,
                        applySubs: rule.applySubs,
                        inherited: rule.inherited,
                        userId: rule.userId,
                        group: rule.group,
                        grant: rule.grant,
                        deny: rule.deny,
                      ),
                    ),
                  ),
                  _Flag(
                    label: l.aclApplySubs,
                    value: rule.applySubs,
                    onChanged: (v) => onChanged!(
                      UiAclRule(
                        applyHere: rule.applyHere,
                        applySubs: v,
                        inherited: rule.inherited,
                        userId: rule.userId,
                        group: rule.group,
                        grant: rule.grant,
                        deny: rule.deny,
                      ),
                    ),
                  ),
                ],
              ),
          ],
        ),
      ),
    );
  }

  /// The groups every Mumble server defines, in words rather than keywords.
  static String _groupLabel(L l, String group) => switch (group) {
    'all' => l.aclEverybody,
    'auth' => l.aclRegistered,
    final other => other,
  };
}

class _PermissionChip extends StatelessWidget {
  const _PermissionChip({
    required this.label,
    required this.stand,
    required this.onTap,
  });

  final String label;
  final PermissionStand stand;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final (colour, icon) = switch (stand) {
      PermissionStand.granted => (StatusColors.connected, Icons.check),
      PermissionStand.denied => (StatusColors.failed, Icons.block),
      PermissionStand.unset => (scheme.onSurfaceVariant, Icons.remove),
    };
    return InkWell(
      onTap: onTap,
      borderRadius: BorderRadius.circular(14),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 4),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: colour.withValues(alpha: 0.6)),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 13, color: colour),
            const SizedBox(width: 4),
            Text(label, style: TextStyle(fontSize: 11, color: colour)),
          ],
        ),
      ),
    );
  }
}

class _Flag extends StatelessWidget {
  const _Flag({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final bool value;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Checkbox(
        value: value,
        visualDensity: VisualDensity.compact,
        onChanged: (v) => onChanged(v ?? false),
      ),
      Text(label, style: const TextStyle(fontSize: 12)),
      const SizedBox(width: 8),
    ],
  );
}
