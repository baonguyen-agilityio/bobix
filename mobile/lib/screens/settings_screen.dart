import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../features/actions/action_widgets.dart' show bioName;
import '../l10n/l10n.dart';
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
    final t = context.tokens;
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
                trailing: _DisconnectButton(computer: computer.name),
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
                value: settings.requireUnlock,
                onChanged: (v) =>
                    ref.read(settingsProvider.notifier).setRequireUnlock(v),
              ),
              toggle: true,
              toggled: settings.requireUnlock,
              onTap: () => ref
                  .read(settingsProvider.notifier)
                  .setRequireUnlock(!settings.requireUnlock),
            ),
            _Row(
              icon: Icons.shield_outlined,
              title: s('phone.settings.never'),
              subtitle: s('phone.settings.never.d'),
            ),
          ],
        ),
        if (computer != null) ...[
          _Section(s('phone.settings.may', {'c': computer.name})),
          _Group(
            children: [
              for (final (name, granted) in _rights(computer))
                _Row(
                  icon: granted ? Icons.check : Icons.close,
                  iconColor: granted ? t.ok : t.ink3,
                  title: s('phone.right.$name'),
                  muted: !granted,
                ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.only(top: 6, left: 4),
            child: Text(
              s('phone.settings.may.n'),
              style: TextStyle(fontSize: 12, color: t.ink3),
            ),
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

  /// The rights the computer granted this phone, in the order Settings lists them.
  List<(String, bool)> _rights(PairedComputer c) {
    final r = c.rights;
    return [
      ('answer', r.answer),
      ('plan', r.plan),
      ('question', r.question),
      ('start', r.start),
      ('stop', r.stop),
      ('activity', r.activity),
    ];
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
class _DisconnectSheet extends ConsumerWidget {
  const _DisconnectSheet({required this.computer});
  final String computer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
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
              const SizedBox(height: 16),
              SizedBox(
                width: double.infinity,
                child: PillButton(
                  label: s('phone.settings.disc.go'),
                  danger: true,
                  primary: false,
                  onPressed: () {
                    Navigator.of(context).pop();
                    ref.read(linkControllerProvider.notifier).disconnect();
                  },
                ),
              ),
              const SizedBox(height: 8),
              SizedBox(
                width: double.infinity,
                child: PillButton(
                  label: s('phone.a.cancel'),
                  onPressed: () => Navigator.of(context).pop(),
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
