import 'dart:typed_data';

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

  /// The rider's edits, for tests.
  ///
  /// Null until they change something, which is the same thing the field
  /// means; what reaches the server is this list, whole, and the shape of it
  /// is what the group tests are about.
  @visibleForTesting
  UiChannelAcl? get edits => _draft;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_asked) return;
    _asked = true;
    final state = AppStateScope.of(context);
    state.loadAcl(widget.serverId, widget.channel.id);
    // The names to offer when adding somebody to a group. Only registered
    // riders can be in one — a group holds account numbers, and somebody with
    // no account has no number to hold.
    state.loadRegistered(widget.serverId);
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
              // **Hidden when they do not apply.** The switch above decides
              // whether this channel takes the parent's rules at all; with it
              // off they are not merely uneditable, they are not in force —
              // and a list that still shows them is a list that says this
              // channel grants things it does not.
              for (final (i, rule) in draft.rules.indexed)
                if (draft.inheritAcls || !rule.inherited)
                  _RuleCard(
                  rule: rule,
                  names: state.runtimeFor(widget.serverId).userNames,
                  groups: draft.groups,
                  registered: state.runtimeFor(widget.serverId).registered,
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
                // **Who it is about comes first.** A rule always arrived
                // addressed to everybody and could never be readdressed, so
                // the one thing a rule is for — naming somebody — was the one
                // thing this screen could not do.
                onPressed: () async {
                  final who = await askRuleSubject(
                    context,
                    groups: draft.groups,
                    registered: state.runtimeFor(widget.serverId).registered,
                  );
                  if (who == null || !mounted) return;
                  setState(() {
                    _draft = _withRules(draft, [
                      ...draft.rules,
                      UiAclRule(
                        applyHere: true,
                        applySubs: true,
                        inherited: false,
                        userId: who.userId,
                        group: who.group,
                        grant: 0,
                        deny: 0,
                      ),
                    ]);
                  });
                },
              ),
              const Divider(height: 24),
              Text(l.aclGroups, style: _heading(context)),
              Text(l.aclGroupsHint, style: _quiet(context)),
              const SizedBox(height: 8),
              for (final (i, g) in draft.groups.indexed)
                _GroupCard(
                  group: g,
                  names: state.runtimeFor(widget.serverId).userNames,
                  registered: state.runtimeFor(widget.serverId).registered,
                  onChanged: g.inherited
                      ? null
                      : (next) => setState(() {
                          final groups = [...draft.groups];
                          groups[i] = next;
                          _draft = _withGroups(draft, groups);
                        }),
                  onRemove: g.inherited
                      ? null
                      : () => setState(() {
                          final groups = [...draft.groups]..removeAt(i);
                          _draft = _withGroups(draft, groups);
                        }),
                ),
              const SizedBox(height: 8),
              OutlinedButton.icon(
                icon: const Icon(Icons.group_add_outlined),
                label: Text(l.aclGroupNew),
                onPressed: () async {
                  final name = await _askGroupName(context, draft);
                  if (name == null || !mounted) return;
                  setState(() {
                    _draft = _withGroups(draft, [
                      ...draft.groups,
                      UiAclGroup(
                        name: name,
                        inherited: false,
                        // What a new group means by default: it takes the
                        // parent's members of the same name, and channels
                        // below may take ours. Both are the server's own
                        // defaults for a group created in Mumble's client.
                        inherit: true,
                        inheritable: true,
                        add: Uint32List(0),
                        remove: Uint32List(0),
                        inheritedMembers: Uint32List(0),
                      ),
                    ]);
                  });
                },
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

  UiChannelAcl _withGroups(UiChannelAcl acl, List<UiAclGroup> groups) =>
      UiChannelAcl(
        channelId: acl.channelId,
        inheritAcls: acl.inheritAcls,
        groups: groups,
        rules: acl.rules,
      );

  /// Asks for a name, refusing an empty one and one already taken.
  ///
  /// **A second group of the same name is not a second group.** The server
  /// keys them by name, so saving two leaves one, and which one is anybody's
  /// guess — better to say so here than to have members quietly disappear.
  Future<String?> _askGroupName(BuildContext context, UiChannelAcl acl) {
    final l = L.of(context);
    final controller = TextEditingController();
    final taken = {for (final g in acl.groups) g.name.toLowerCase()};
    return showDialog<String>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setLocal) {
          final text = controller.text.trim();
          final error = switch (text) {
            '' => null,
            final t when taken.contains(t.toLowerCase()) => l.aclGroupExists,
            _ => null,
          };
          return AlertDialog(
            title: Text(l.aclGroupNew),
            content: TextField(
              controller: controller,
              autofocus: true,
              decoration: InputDecoration(
                labelText: l.aclGroupName,
                errorText: error,
              ),
              onChanged: (_) => setLocal(() {}),
              onSubmitted: (v) {
                final name = v.trim();
                if (name.isEmpty || taken.contains(name.toLowerCase())) return;
                Navigator.pop(context, name);
              },
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text(l.cancel),
              ),
              FilledButton(
                onPressed: text.isEmpty || error != null
                    ? null
                    : () => Navigator.pop(context, text),
                child: Text(l.aclGroupCreate),
              ),
            ],
          );
        },
      ),
    );
  }
}

/// One group: who is in it, who was taken out of it, and where it reaches.
///
/// **Three lists, not one.** Mumble keeps a group's membership as what this
/// channel *adds*, what it *removes* from whatever the parent passed down, and
/// the inherited members themselves. So taking somebody out of an inherited
/// group is not deleting them — it is adding them to the remove list, and the
/// chip says so rather than vanishing, because a rider who disappears from a
/// list reads as a mistake rather than as a decision.
class _GroupCard extends StatelessWidget {
  const _GroupCard({
    required this.group,
    required this.names,
    required this.registered,
    required this.onChanged,
    required this.onRemove,
  });

  final UiAclGroup group;
  final Map<int, String> names;
  final List<UiRegisteredUser> registered;
  final void Function(UiAclGroup)? onChanged;
  final VoidCallback? onRemove;

  String _name(int id) =>
      names[id] ??
      registered.where((r) => r.userId == id).map((r) => r.name).firstOrNull ??
      '#$id';

  /// The bridge hands these over as `Uint32List`, so every edit is rebuilt
  /// as one rather than as the `List<int>` that reads more naturally here.
  UiAclGroup _with({
    bool? inherit,
    bool? inheritable,
    List<int>? add,
    List<int>? remove,
  }) => UiAclGroup(
    name: group.name,
    inherited: group.inherited,
    inherit: inherit ?? group.inherit,
    inheritable: inheritable ?? group.inheritable,
    add: add == null ? group.add : Uint32List.fromList(add),
    remove: remove == null ? group.remove : Uint32List.fromList(remove),
    inheritedMembers: group.inheritedMembers,
  );

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final scheme = Theme.of(context).colorScheme;
    final editable = onChanged != null;
    final taken = {...group.add, ...group.inheritedMembers, ...group.remove};

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
                    group.name,
                    style: const TextStyle(fontWeight: FontWeight.w600),
                  ),
                ),
                if (group.inherited)
                  Text(
                    l.aclInherited,
                    style: TextStyle(
                      fontSize: 11,
                      color: scheme.onSurfaceVariant,
                    ),
                  )
                else if (onRemove != null)
                  IconButton(
                    tooltip: l.aclGroupRemove,
                    icon: const Icon(Icons.delete_outline, size: 18),
                    onPressed: onRemove,
                  ),
              ],
            ),
            if (group.add.isEmpty &&
                group.inheritedMembers.isEmpty &&
                group.remove.isEmpty)
              Text(l.aclGroupNobody, style: _quiet(context))
            else
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final id in group.add)
                    _MemberChip(
                      label: _name(id),
                      onRemove: editable
                          ? () => onChanged!(
                              _with(
                                add: [...group.add]..remove(id),
                              ),
                            )
                          : null,
                    ),
                  for (final id in group.inheritedMembers)
                    if (!group.remove.contains(id))
                      _MemberChip(
                        label: _name(id),
                        inherited: true,
                        onRemove: editable
                            ? () => onChanged!(
                                _with(remove: [...group.remove, id]),
                              )
                            : null,
                      ),
                  for (final id in group.remove)
                    _MemberChip(
                      label: _name(id),
                      excluded: true,
                      onUndo: editable
                          ? () => onChanged!(
                              _with(
                                remove: [...group.remove]..remove(id),
                              ),
                            )
                          : null,
                    ),
                ],
              ),
            if (editable) ...[
              const SizedBox(height: 4),
              Row(
                children: [
                  TextButton.icon(
                    icon: const Icon(Icons.person_add_alt, size: 18),
                    label: Text(l.aclGroupAddMember),
                    onPressed: () async {
                      final id = await _pickMember(context, taken);
                      if (id == null) return;
                      onChanged!(
                        _with(
                          add: [...group.add, id],
                          remove: [...group.remove]..remove(id),
                        ),
                      );
                    },
                  ),
                ],
              ),
              SwitchListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: group.inherit,
                title: Text(l.aclGroupInherit, style: const TextStyle(fontSize: 13)),
                onChanged: (v) => onChanged!(_with(inherit: v)),
              ),
              SwitchListTile(
                dense: true,
                contentPadding: EdgeInsets.zero,
                value: group.inheritable,
                title: Text(
                  l.aclGroupInheritable,
                  style: const TextStyle(fontSize: 13),
                ),
                onChanged: (v) => onChanged!(_with(inheritable: v)),
              ),
            ],
          ],
        ),
      ),
    );
  }

  /// The registered riders, minus the ones already accounted for here.
  Future<int?> _pickMember(BuildContext context, Set<int> taken) {
    final l = L.of(context);
    final choices = [
      for (final r in registered)
        if (!taken.contains(r.userId)) r,
    ]..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    return showDialog<int>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.aclGroupAddMember),
        content: SizedBox(
          width: 320,
          child: choices.isEmpty
              ? Text(l.aclGroupNobodyToAdd)
              : ListView(
                  shrinkWrap: true,
                  children: [
                    for (final r in choices)
                      ListTile(
                        dense: true,
                        title: Text(r.name),
                        onTap: () => Navigator.pop(context, r.userId),
                      ),
                  ],
                ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(l.cancel),
          ),
        ],
      ),
    );
  }

  TextStyle _quiet(BuildContext context) => TextStyle(
    fontSize: 12,
    color: Theme.of(context).colorScheme.onSurfaceVariant,
  );
}

/// One rider in a group: theirs, the parent's, or taken out of the parent's.
class _MemberChip extends StatelessWidget {
  const _MemberChip({
    required this.label,
    this.inherited = false,
    this.excluded = false,
    this.onRemove,
    this.onUndo,
  });

  final String label;
  final bool inherited;
  final bool excluded;
  final VoidCallback? onRemove;
  final VoidCallback? onUndo;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Chip(
      visualDensity: VisualDensity.compact,
      avatar: Icon(
        excluded
            ? Icons.person_off_outlined
            : inherited
            ? Icons.arrow_downward
            : Icons.person_outline,
        size: 16,
        color: excluded ? scheme.error : scheme.onSurfaceVariant,
      ),
      label: Text(
        label,
        style: TextStyle(
          fontSize: 12,
          decoration: excluded ? TextDecoration.lineThrough : null,
          color: excluded ? scheme.onSurfaceVariant : null,
        ),
      ),
      onDeleted: onRemove ?? onUndo,
      deleteIcon: Icon(onUndo != null ? Icons.undo : Icons.close, size: 15),
    );
  }
}

/// One rule: who it is about, where it applies, and what it says.
class _RuleCard extends StatelessWidget {
  const _RuleCard({
    required this.rule,
    required this.names,
    required this.groups,
    required this.registered,
    required this.onChanged,
    required this.onRemove,
  });

  final UiAclRule rule;
  final Map<int, String> names;

  /// What this channel's access list defines, so a rule can name one.
  final List<UiAclGroup> groups;

  /// Who has an account here, so a rule can name one of them instead.
  final List<UiRegisteredUser> registered;
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
                  // Tap to readdress it. The subject is as much a part of a
                  // rule as the permissions are, and it used to be the only
                  // part fixed at birth.
                  child: InkWell(
                    onTap: onChanged == null
                        ? null
                        : () async {
                            final next = await askRuleSubject(
                              context,
                              groups: groups,
                              registered: registered,
                            );
                            if (next == null) return;
                            onChanged!(
                              UiAclRule(
                                applyHere: rule.applyHere,
                                applySubs: rule.applySubs,
                                inherited: false,
                                userId: next.userId,
                                group: next.group,
                                grant: rule.grant,
                                deny: rule.deny,
                              ),
                            );
                          },
                    child: Row(
                      children: [
                        Flexible(
                          child: Text(
                            who,
                            style: const TextStyle(fontWeight: FontWeight.w600),
                          ),
                        ),
                        if (onChanged != null) ...[
                          const SizedBox(width: 4),
                          Icon(
                            Icons.edit_outlined,
                            size: 14,
                            color: scheme.onSurfaceVariant,
                          ),
                        ],
                      ],
                    ),
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

  static String _groupLabel(L l, String group) => groupLabel(l, group);
}

/// Who a rule is about: one of the groups, or one registered rider.
class RuleSubject {
  const RuleSubject({this.group, this.userId});

  final String? group;
  final int? userId;
}

/// Asks who a rule should be about.
///
/// **Both halves are offered, because the protocol has both.** An access list
/// entry names either a group — the built-in ones every server has, or one
/// this channel defines — or a single registered rider by account id. The
/// built-ins come first because they are what most rules use; the riders are
/// listed after them, and only those with an account, since an id is what a
/// rule can hold.
Future<RuleSubject?> askRuleSubject(
  BuildContext context, {
  required List<UiAclGroup> groups,
  required List<UiRegisteredUser> registered,
}) {
  final l = L.of(context);
  // The five Mumble defines itself, in the order its own client lists them.
  const builtIn = ['all', 'auth', 'in', 'out', 'sub'];
  final named = <String>[
    ...builtIn,
    for (final g in groups)
      if (!builtIn.contains(g.name)) g.name,
  ];
  final riders = [...registered]
    ..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));

  return showDialog<RuleSubject>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l.aclRuleFor),
      content: SizedBox(
        width: 340,
        child: ListView(
          shrinkWrap: true,
          children: [
            for (final g in named)
              ListTile(
                dense: true,
                leading: const Icon(Icons.groups_outlined, size: 20),
                title: Text(groupLabel(l, g)),
                onTap: () => Navigator.pop(context, RuleSubject(group: g)),
              ),
            if (riders.isNotEmpty) const Divider(),
            for (final r in riders)
              ListTile(
                dense: true,
                leading: const Icon(Icons.person_outline, size: 20),
                title: Text(r.name),
                onTap: () =>
                    Navigator.pop(context, RuleSubject(userId: r.userId)),
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: Text(l.cancel),
        ),
      ],
    ),
  );
}

/// The groups every Mumble server defines, in words rather than keywords.
String groupLabel(L l, String group) => switch (group) {
  'all' => l.aclEverybody,
  'auth' => l.aclRegistered,
  'in' => l.aclGroupIn,
  'out' => l.aclGroupOut,
  'sub' => l.aclGroupSub,
  final other => other,
};

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
