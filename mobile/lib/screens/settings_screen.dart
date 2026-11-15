import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../features/actions/action_widgets.dart' show bioName;
import '../keys/platform_secure_keys.dart';
import '../keys/secure_keys.dart';
import '../l10n/l10n.dart';
import '../push/hide_details.dart';
import '../src/rust/api/messages.dart' show PhoneRights;
import '../link/link_providers.dart';
import '../link/link_state.dart';
import '../settings/app_settings.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/glass.dart';
import 'feedback_screen.dart';

/// The day and month of [ms] in the person's language ("11 Nov", "11 thg 11").
String shortDate(int ms, Strings s) {
  final d = DateTime.fromMillisecondsSinceEpoch(ms);
  return '${d.day} ${s('phone.mon.${d.month}')}';
}

/// Settings: the computer this phone is paired with and what it may do there, disconnect, the safety rows, the look and
/// the language, and "Send feedback".
class SettingsScreen extends ConsumerWidget {
  const SettingsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final computer = ref.watch(pairedComputerProvider);
    final status = ref.watch(linkStatusProvider).value;
    final settings = ref.watch(settingsProvider).value ?? const AppSettings();
    final lang = ref.watch(langProvider);

    String state(ConnState? c) => switch (c) {
      ConnState.ok => s('phone.settings.online'),
      ConnState.asleep => s('phone.settings.asleep'),
      ConnState.offline => s('phone.settings.offline'),
      ConnState.noNetwork ||
      ConnState.relayDown => s('phone.settings.unreachable'),
      null => s('phone.settings.unreachable'),
    };

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        _Title(s('phone.settings.title')),
        if (computer != null) ...[
          _Section(s('phone.settings.comps')),
          _Group(
            children: [
              _Row(
                icon: Icons.computer,
                title: computer.name,
                subtitle: [
                  if (computer.pairedAt != null)
                    s('phone.settings.paired', {
                      'd': shortDate(computer.pairedAt!, s),
                    }),
                  state(status?.conn),
                ].join(' · '),
                below: _DisconnectButton(computer: computer.name),
              ),
              _Row(
                icon: Icons.qr_code_scanner,
                title: s('phone.settings.another'),
                subtitle: s('phone.settings.another.d'),
                muted: true,
              ),
            ],
          ),
        ],
        _Section(s('phone.settings.sec')),
        _Group(
          children: [
            _Row(
              icon: Icons.lock_outline,
              title: s('phone.settings.open', {'os': bioName(s)}),
              subtitle: s('phone.settings.open.d'),
              trailing: Switch(
                key: const Key('lock-switch'),
                value: settings.requireUnlock,
                onChanged: (v) => _setLock(ref, s, v),
              ),
              toggle: true,
              toggled: settings.requireUnlock,
              onTap: () => _setLock(ref, s, !settings.requireUnlock),
            ),
            const HideDetailsSwitch(),
            _Row(
              icon: Icons.shield_outlined,
              title: s('phone.settings.never'),
              subtitle: s('phone.settings.never.d'),
            ),
          ],
        ),
        if (computer != null) ...[
          _Section(s('phone.perm.sec')),
          _Group(
            children: [
              ValueListenableBuilder<PhoneRights>(
                valueListenable: computer.rightsListenable,
                builder: (context, granted, _) {
                  final rights = _rights(granted);
                  return _Row(
                    icon: Icons.shield_outlined,
                    title: s('phone.perm.title'),
                    subtitle: s('phone.perm.sum', {
                      'n': '${rights.where((r) => r.$2).length}',
                      'all': '${rights.length}',
                      'c': computer.name,
                    }),
                    chevron: true,
                    onTap: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => const PermissionsScreen(),
                      ),
                    ),
                  );
                },
              ),
            ],
          ),
        ],
        _Section(s('phone.settings.look')),
        _Picker(
          label: s('phone.settings.theme'),
          children: [
            for (final v in ThemeChoice.values)
              _Choice(
                label: s('phone.settings.${v.name}'),
                selected: settings.theme == v,
                onTap: () => ref.read(settingsProvider.notifier).setTheme(v),
              ),
          ],
        ),
        _Section(s('phone.settings.lang')),
        _Picker(
          label: s('phone.settings.lang'),
          children: [
            for (final (code, name) in const [
              ('en', 'English'),
              ('vi', 'Tiếng Việt'),
            ])
              _Choice(
                label: name,
                selected: lang == code,
                onTap: () =>
                    ref.read(settingsProvider.notifier).setLanguage(code),
              ),
          ],
        ),
        const SizedBox(height: 18),
        _Group(
          children: [
            _Row(
              icon: Icons.feedback_outlined,
              title: s('phone.settings.fb'),
              subtitle: s('phone.settings.fb.d'),
              chevron: true,
              onTap: () => Navigator.of(context).push(
                MaterialPageRoute<void>(builder: (_) => const FeedbackScreen()),
              ),
            ),
          ],
        ),
      ],
    );
  }

  /// Turning the lock on needs nothing; turning it off asks first, so whoever holds the open phone cannot drop it.
  Future<void> _setLock(WidgetRef ref, Strings s, bool on) async {
    if (!on) {
      final answer = await ref
          .read(secureKeysProvider)
          .authenticate(reason: s('phone.settings.off.reason'));
      // A phone with nothing to ask with has no lock to protect.
      if (answer != AppAuth.ok && answer != AppAuth.unavailable) return;
    }
    await ref.read(settingsProvider.notifier).setRequireUnlock(on);
  }
}

/// The rights the computer granted this phone, in the order Settings lists them.
List<(String, bool)> _rights(PhoneRights r) {
  return [
    ('answer', r.answer),
    ('plan', r.plan),
    ('question', r.question),
    ('start', r.start),
    ('stop', r.stop),
    ('activity', r.activity),
  ];
}

/// "What this phone may do": what the computer allowed and did not allow, and what always holds. Read only: the
/// rights are set on the computer.
class PermissionsScreen extends ConsumerWidget {
  const PermissionsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final computer = ref.watch(pairedComputerProvider);
    if (computer == null) {
      return DetailScaffold(title: s('phone.perm.title'), children: const []);
    }
    return ValueListenableBuilder<PhoneRights>(
      valueListenable: computer.rightsListenable,
      builder: (context, granted, _) =>
          _permissions(context, s, computer, _rights(granted)),
    );
  }

  Widget _permissions(
    BuildContext context,
    Strings s,
    PairedComputer computer,
    List<(String, bool)> rights,
  ) {
    final t = context.tokens;
    final on = [
      for (final r in rights)
        if (r.$2) r.$1,
    ];
    final off = [
      for (final r in rights)
        if (!r.$2) r.$1,
    ];
    _Row row(String name, bool granted) => _Row(
      icon: granted ? Icons.check : Icons.close,
      iconColor: granted ? t.ok : t.ink3,
      title: s('phone.right.$name'),
      subtitle: s('phone.perm.$name.d'),
      muted: !granted,
    );
    return DetailScaffold(
      title: s('phone.perm.title'),
      children: [
        _Group(
          children: [
            _Row(
              icon: Icons.computer,
              title: computer.name,
              subtitle: s('phone.perm.who', {'p': computer.phoneName}),
              trailing: Text(
                '${on.length}/${rights.length}',
                style: TextStyle(fontWeight: FontWeight.w600, color: t.ink2),
              ),
            ),
          ],
        ),
        _Section(s('phone.perm.on')),
        if (on.isEmpty)
          Padding(
            padding: const EdgeInsets.only(left: 4),
            child: Text(
              s('phone.perm.none'),
              style: TextStyle(fontSize: 14, color: t.ink2),
            ),
          )
        else
          _Group(children: [for (final n in on) row(n, true)]),
        if (off.isNotEmpty) ...[
          _Section(s('phone.perm.off')),
          _Group(children: [for (final n in off) row(n, false)]),
        ],
        _Section(s('phone.perm.always')),
        _Group(
          children: [
            _Row(
              icon: Icons.fingerprint,
              title: s('phone.perm.bio', {'os': bioName(s)}),
              subtitle: s('phone.perm.bio.d'),
            ),
            _Row(
              icon: Icons.shield_outlined,
              title: s('phone.settings.never'),
              subtitle: s('phone.settings.never.d'),
            ),
          ],
        ),
        Padding(
          padding: const EdgeInsets.only(top: 12, left: 4),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.computer, size: 16, color: t.ink3),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  s('phone.perm.note'),
                  style: TextStyle(fontSize: 12, height: 1.35, color: t.ink3),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _Title extends StatelessWidget {
  const _Title(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(0, 6, 0, 12),
    child: Semantics(
      header: true,
      child: Text(
        text,
        style: TextStyle(
          fontSize: 28,
          fontWeight: FontWeight.w700,
          letterSpacing: -.5,
          color: context.tokens.ink,
        ),
      ),
    ),
  );
}

class _Section extends StatelessWidget {
  const _Section(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(4, 18, 0, 8),
    child: Semantics(
      header: true,
      child: Text(
        text,
        style: TextStyle(
          fontSize: 13,
          fontWeight: FontWeight.w600,
          color: context.tokens.ink3,
        ),
      ),
    ),
  );
}

class _Group extends StatelessWidget {
  const _Group({required this.children});
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Glass(
      child: Material(
        type: MaterialType.transparency,
        child: Column(
          children: [
            for (var i = 0; i < children.length; i++) ...[
              if (i > 0) Divider(height: .5, thickness: .5, color: t.line),
              children[i],
            ],
          ],
        ),
      ),
    );
  }
}

/// One line of a group: an icon, a title and a second line, and what sits at the end.
class _Row extends StatelessWidget {
  const _Row({
    required this.icon,
    required this.title,
    this.subtitle,
    this.trailing,
    this.below,
    this.onTap,
    this.iconColor,
    this.muted = false,
    this.chevron = false,
    this.toggle = false,
    this.toggled = false,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final Widget? trailing;

  /// Under the text, on its own line, so a large font never pushes it out of the row.
  final Widget? below;
  final VoidCallback? onTap;
  final Color? iconColor;
  final bool muted, chevron, toggle, toggled;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final row = Opacity(
      opacity: muted ? .6 : 1,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 60),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Row(
            children: [
              Icon(icon, size: 22, color: iconColor ?? t.ink2),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w500,
                        color: t.ink,
                      ),
                    ),
                    if (subtitle != null)
                      Text(
                        subtitle!,
                        style: TextStyle(
                          fontSize: 12.5,
                          height: 1.35,
                          color: t.ink3,
                        ),
                      ),
                    if (below != null)
                      Padding(
                        padding: const EdgeInsets.only(top: 8),
                        child: below!,
                      ),
                  ],
                ),
              ),
              ?trailing,
              if (chevron) Icon(Icons.chevron_right, color: t.ink3),
            ],
          ),
        ),
      ),
    );
    if (onTap == null) return row;
    return Semantics(
      toggled: toggle ? toggled : null,
      button: !toggle,
      child: InkWell(
        onTap: onTap,
        child: ExcludeSemantics(child: row),
      ),
    );
  }
}

class _DisconnectButton extends ConsumerWidget {
  const _DisconnectButton({required this.computer});
  final String computer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return PillButton(
      label: s('phone.settings.disc'),
      danger: true,
      compact: true,
      onPressed: () => showModalBottomSheet<void>(
        context: context,
        isScrollControlled: true,
        backgroundColor: Colors.transparent,
        builder: (_) => _DisconnectSheet(computer: computer),
      ),
    );
  }
}

/// "Disconnect from {computer}?": says what happens, and the Erase that wipes the keys and the log.
class _DisconnectSheet extends ConsumerStatefulWidget {
  const _DisconnectSheet({required this.computer});
  final String computer;

  @override
  ConsumerState<_DisconnectSheet> createState() => _DisconnectSheetState();
}

class _DisconnectSheetState extends ConsumerState<_DisconnectSheet> {
  bool _busy = false;
  bool _failed = false;

  Future<void> _go() async {
    setState(() {
      _busy = true;
      _failed = false;
    });
    try {
      await ref.read(linkControllerProvider.notifier).disconnect();
      if (mounted) Navigator.of(context).pop();
    } on Object {
      // The keys could not be erased (the Keychain refused): say so, and let the person try again.
      if (mounted) {
        setState(() {
          _busy = false;
          _failed = true;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final computer = widget.computer;
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(8),
        child: Container(
          padding: const EdgeInsets.fromLTRB(20, 14, 20, 16),
          decoration: BoxDecoration(
            color: t.surface,
            borderRadius: BorderRadius.circular(32),
            border: Border.all(width: .5, color: t.line),
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Semantics(
                header: true,
                child: Text(
                  s('phone.settings.disc.h', {'c': computer}),
                  textAlign: TextAlign.center,
                  style: TextStyle(
                    fontSize: 18,
                    fontWeight: FontWeight.w700,
                    color: t.ink,
                  ),
                ),
              ),
              const SizedBox(height: 8),
              Text(
                s('phone.settings.disc.d'),
                textAlign: TextAlign.center,
                style: TextStyle(fontSize: 14, height: 1.4, color: t.ink2),
              ),
              if (_failed)
                Padding(
                  padding: const EdgeInsets.only(top: 10),
                  child: Text(
                    s('phone.settings.disc.err'),
                    textAlign: TextAlign.center,
                    style: TextStyle(fontSize: 13, color: t.err),
                  ),
                ),
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                child: PillButton(
                  label: s('phone.settings.disc.go'),
                  danger: true,
                  primary: false,
                  onPressed: _busy ? null : _go,
                ),
              ),
              const SizedBox(height: 8),
              SizedBox(
                width: double.infinity,
                child: PillButton(
                  label: s('phone.a.cancel'),
                  onPressed: _busy ? null : () => Navigator.of(context).pop(),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Picker extends StatelessWidget {
  const _Picker({required this.label, required this.children});
  final String label;
  final List<_Choice> children;

  @override
  Widget build(BuildContext context) => Semantics(
    label: label,
    container: true,
    child: Row(
      children: [
        for (var i = 0; i < children.length; i++) ...[
          if (i > 0) const SizedBox(width: 10),
          Expanded(child: children[i]),
        ],
      ],
    ),
  );
}

class _Choice extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.selected,
    required this.onTap,
  });
  final String label;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    return Semantics(
      inMutuallyExclusiveGroup: true,
      selected: selected,
      button: true,
      label: label,
      child: ExcludeSemantics(
        child: Material(
          color: Colors.transparent,
          child: InkWell(
            borderRadius: BorderRadius.circular(16),
            onTap: onTap,
            child: Glass(
              tint: selected ? t.accentBg : null,
              border: selected ? t.accent : null,
              padding: const EdgeInsets.symmetric(vertical: 16, horizontal: 8),
              child: ConstrainedBox(
                constraints: const BoxConstraints(minHeight: 44),
                child: Center(
                  child: Text(
                    label,
                    textAlign: TextAlign.center,
                    style: TextStyle(
                      fontSize: 14,
                      fontWeight: FontWeight.w600,
                      color: selected ? t.accentInk : t.ink2,
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
