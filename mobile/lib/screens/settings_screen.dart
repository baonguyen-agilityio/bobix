import 'package:flutter/foundation.dart'
    show defaultTargetPlatform, TargetPlatform;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/models.dart';
import '../features/actions/action_widgets.dart' show bioName;
import '../keys/platform_secure_keys.dart';
import '../keys/secure_keys.dart';
import '../l10n/l10n.dart';
import '../log/activity_log.dart';
import '../push/hide_details.dart';
import '../src/rust/api/messages.dart' show PhoneRights;
import '../link/link_providers.dart';
import '../link/link_state.dart';
import '../settings/app_settings.dart';
import '../theme/theme.dart' show minTouch;
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/floating_sheet.dart';
import '../widgets/glass.dart';
import '../widgets/switch_row.dart';
import 'app_log_screen.dart';
import 'feedback_screen.dart';
import 'widgets/widgets_screen.dart';

export 'widgets/widgets_screen.dart' show WidgetsScreen;

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

    final logCount = ref.watch(appLogProvider).length;
    void open(Widget screen) =>
        Navigator.of(context)
            .push(MaterialPageRoute<void>(builder: (_) => screen));

    return ListView(
      padding: const EdgeInsets.fromLTRB(16, 4, 16, 96),
      children: [
        _Title(s('phone.settings.title')),
        if (computer != null) ...[
          _Section(s('phone.settings.comps')),
          _Group(
            children: [
              _Row(
                icon: Icons.monitor,
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
                icon: Icons.qr_code_2,
                title: s('phone.settings.another'),
                subtitle: s('phone.settings.another.d'),
                accent: true,
              ),
            ],
          ),
        ],
        _Section(s('phone.settings.sec')),
        _Group(
          children: [
            _Row(
              icon: bioIcon(),
              title: s('phone.perm.bio', {'os': bioName(s)}),
              subtitle: s('phone.perm.bio.d'),
              trailing: _AlwaysOn(s('phone.settings.always')),
            ),
            SwitchRow(
              switchKey: const Key('lock-switch'),
              title: s('phone.settings.open', {'os': bioName(s)}),
              subtitle: s('phone.settings.open.d'),
              value: settings.requireUnlock,
              onChanged: (v) => _setLock(ref, s, v),
            ),
            _Row(
              icon: Icons.shield_outlined,
              title: s('phone.settings.never'),
              subtitle: s('phone.settings.never.d'),
            ),
            const HideDetailsSwitch(),
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
                    onTap: () => open(const PermissionsScreen()),
                  );
                },
              ),
            ],
          ),
        ],
        _Section(s('phone.settings.wg')),
        _Group(
          children: [
            _Row(
              icon: Icons.pets_outlined,
              title: s('phone.settings.wg'),
              subtitle: s('phone.settings.wg.d'),
              chevron: true,
              onTap: () => open(const WidgetsScreen()),
            ),
          ],
        ),
        _Section(s('phone.settings.log')),
        _Group(
          children: [
            _Row(
              icon: Icons.format_list_bulleted,
              title: s('phone.settings.log'),
              subtitle: s('phone.settings.log.d'),
              trailing: CountBadge(logCount),
              chevron: true,
              onTap: () => open(const AppLogScreen()),
            ),
          ],
        ),
        _Section(s('phone.settings.look')),
        _Picker(
          label: s('phone.settings.theme'),
          children: [
            for (final v in ThemeChoice.values)
              _Choice(
                label: s('phone.settings.${v.name}'),
                selected: settings.theme == v,
                swatch: _ThemeSwatch(v),
                onTap: () => ref.read(settingsProvider.notifier).setTheme(v),
              ),
          ],
        ),
        _Section(s('phone.settings.lang')),
        _Picker(
          label: s('phone.settings.lang'),
          children: [
            for (final (code, name, hello) in const [
              ('en', 'English', 'Hello'),
              ('vi', 'Tiếng Việt', 'Xin chào'),
            ])
              _Choice(
                label: name,
                selected: lang == code,
                swatch: _LangSwatch(
                  code: code.toUpperCase(),
                  hello: hello,
                  selected: lang == code,
                ),
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
              onTap: () => open(const FeedbackScreen()),
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

/// The icon of the phone's own check: a face on iPhone, a fingerprint elsewhere.
IconData bioIcon() => defaultTargetPlatform == TargetPlatform.iOS
    ? Icons.face_unlock_outlined
    : Icons.fingerprint;

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
      tile: granted ? _Tile.ok : _Tile.plain,
      title: s('phone.right.$name'),
      subtitle: s('phone.perm.$name.d'),
      muted: !granted,
    );
    return DetailScaffold(
      title: s('phone.perm.title'),
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 4, 4, 0),
          child: Row(
            children: [
              Icon(Icons.monitor, size: 28, color: t.ink),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      computer.name,
                      style: TextStyle(
                        fontSize: 16,
                        fontWeight: FontWeight.w700,
                        height: 1.3,
                        color: t.ink,
                      ),
                    ),
                    Text(
                      s('phone.perm.who', {'p': computer.phoneName}),
                      style: TextStyle(fontSize: 13, color: t.ink2),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              _Badge('${on.length}/${rights.length}', good: on.isNotEmpty),
            ],
          ),
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
              icon: bioIcon(),
              tile: _Tile.plain,
              title: s('phone.perm.bio', {'os': bioName(s)}),
              subtitle: s('phone.perm.bio.d'),
            ),
            _Row(
              icon: Icons.shield_outlined,
              tile: _Tile.plain,
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
              Padding(
                padding: const EdgeInsets.only(top: 1),
                child: Icon(Icons.monitor, size: 16, color: t.ink3),
              ),
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
    padding: const EdgeInsets.fromLTRB(0, 0, 0, 0),
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

/// What sits behind a row's icon: nothing, a glass tile, or a green one.
enum _Tile { none, plain, ok }

/// A small rounded label: "Always on", "3/6".
class _Badge extends StatelessWidget {
  const _Badge(this.text, {this.icon, this.good = true});
  final String text;
  final IconData? icon;
  final bool good;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final fg = good ? t.ok : t.ink2;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 1),
      constraints: const BoxConstraints(minHeight: 22),
      decoration: BoxDecoration(
        color: good ? t.okBg : t.surface3,
        borderRadius: BorderRadius.circular(11),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (icon != null) ...[
            Icon(icon, size: 12, color: fg),
            const SizedBox(width: 4),
          ],
          Text(
            text,
            style: TextStyle(
              fontSize: 12,
              fontWeight: FontWeight.w600,
              height: 1.3,
              color: fg,
            ),
          ),
        ],
      ),
    );
  }
}

/// The "Always on" badge of the check before each command: it cannot be turned off.
class _AlwaysOn extends StatelessWidget {
  const _AlwaysOn(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => _Badge(text, icon: Icons.lock_outline);
}

/// One line of a group: an icon, a title and a second line, and what sits at the end.
class _Row extends StatelessWidget {
  const _Row({
    required this.icon,
    required this.title,
    this.subtitle,
    this.trailing,
    this.onTap,
    this.tile = _Tile.none,
    this.muted = false,
    this.accent = false,
    this.chevron = false,
  });

  final IconData icon;
  final String title;
  final String? subtitle;
  final Widget? trailing;
  final VoidCallback? onTap;
  final _Tile tile;
  final bool muted, accent, chevron;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final ink = accent ? t.accentInk : t.ink;
    final lead = tile == _Tile.none
        ? Icon(icon, size: 24, color: accent ? t.accentInk : t.ink)
        : Container(
            width: 44,
            height: 44,
            decoration: BoxDecoration(
              color: tile == _Tile.ok
                  ? t.okBg
                  : (t.isDark ? t.surface3 : const Color(0x99FFFFFF)),
              borderRadius: BorderRadius.circular(12),
              border: tile == _Tile.plain
                  ? Border.all(width: .5, color: t.line)
                  : null,
            ),
            child: Icon(
              icon,
              size: 20,
              color: tile == _Tile.ok ? t.ok : (muted ? t.ink3 : t.ink2),
            ),
          );
    final row = Opacity(
      opacity: muted ? .6 : 1,
      child: ConstrainedBox(
        constraints: const BoxConstraints(minHeight: 60),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Row(
            children: [
              lead,
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
                        color: ink,
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
              if (trailing != null) ...[const SizedBox(width: 8), trailing!],
              if (chevron) Icon(Icons.chevron_right, color: t.ink3),
            ],
          ),
        ),
      ),
    );
    if (onTap == null) return row;
    return Semantics(
      button: true,
      child: InkWell(
        onTap: onTap,
        child: ExcludeSemantics(child: row),
      ),
    );
  }
}

/// The red capsule at the end of the computer's row. It looks 32 pt high and takes a thumb 44 pt high.
class _DisconnectButton extends ConsumerWidget {
  const _DisconnectButton({required this.computer});
  final String computer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final label = s('phone.settings.disc');
    return Semantics(
      button: true,
      label: label,
      excludeSemantics: true,
      child: InkWell(
        borderRadius: BorderRadius.circular(22),
        onTap: () => showModalBottomSheet<void>(
          context: context,
          isScrollControlled: true,
          backgroundColor: Colors.transparent,
          builder: (_) => _DisconnectSheet(computer: computer),
        ),
        child: ConstrainedBox(
          // Capped, so a very large font cuts the label short rather than push the computer's name out.
          constraints: const BoxConstraints(minHeight: minTouch, maxWidth: 170),
          child: Center(
            widthFactor: 1,
            child: Container(
              height: 32,
              padding: const EdgeInsets.symmetric(horizontal: 11),
              decoration: BoxDecoration(
                color: t.err.withValues(alpha: .07),
                borderRadius: BorderRadius.circular(16),
                border: Border.all(
                  width: .5,
                  color: t.err.withValues(alpha: .28),
                ),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(Icons.cloud_off_outlined, size: 16, color: t.err),
                  const SizedBox(width: 4),
                  Flexible(
                    child: Text(
                      label,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w600,
                        color: t.err,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
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
    return FloatingSheet(
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.settings.disc.h', {'c': computer}),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 18,
              fontWeight: FontWeight.w700,
              height: 1.25,
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
        PillButton(
          label: s('phone.settings.disc.go'),
          danger: true,
          primary: false,
          onPressed: _busy ? null : _go,
        ),
        const SizedBox(height: 8),
        PillButton(
          label: s('phone.a.cancel'),
          onPressed: _busy ? null : () => Navigator.of(context).pop(),
        ),
      ],
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
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < children.length; i++) ...[
          if (i > 0) const SizedBox(width: 10),
          Expanded(child: children[i]),
        ],
      ],
    ),
  );
}

/// A glass card of a picker: a swatch of the choice over its name. The chosen one has an accent ring.
class _Choice extends StatelessWidget {
  const _Choice({
    required this.label,
    required this.selected,
    required this.swatch,
    required this.onTap,
  });
  final String label;
  final bool selected;
  final Widget swatch;
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
            child: Stack(
              children: [
                Glass(
                  padding: const EdgeInsets.fromLTRB(6, 6, 6, 9),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      // The swatch grows with the font, so a large one never clips its letters.
                      SizedBox(
                        height: MediaQuery.textScalerOf(context).scale(56),
                        child: swatch,
                      ),
                      const SizedBox(height: 7),
                      Text(
                        label,
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                          height: 1.2,
                          color: selected ? t.accentInk : t.ink2,
                        ),
                      ),
                    ],
                  ),
                ),
                if (selected)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          borderRadius: BorderRadius.circular(16),
                          border: Border.all(width: 1.5, color: t.accentSolid),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

const _lightBg = Color(0xFFF4F5F8);
const _lightInk = Color(0xFF1C1D21);
const _darkBg = Color(0xFF131418);
const _darkInk = Color(0xFFECECEE);

/// "Aa" on the colours of a theme; the system one is split down the middle, half light and half dark.
class _ThemeSwatch extends StatelessWidget {
  const _ThemeSwatch(this.choice);
  final ThemeChoice choice;

  @override
  Widget build(BuildContext context) {
    const aa = TextStyle(fontSize: 17, fontWeight: FontWeight.w600);
    final Widget face = switch (choice) {
      ThemeChoice.light => const ColoredBox(
        color: _lightBg,
        child: Center(
          child: Text(
            'Aa',
            style: TextStyle(
              fontSize: 17,
              fontWeight: FontWeight.w600,
              color: _lightInk,
            ),
          ),
        ),
      ),
      ThemeChoice.dark => const ColoredBox(
        color: _darkBg,
        child: Center(
          child: Text(
            'Aa',
            style: TextStyle(
              fontSize: 17,
              fontWeight: FontWeight.w600,
              color: _darkInk,
            ),
          ),
        ),
      ),
      ThemeChoice.system => Stack(
        fit: StackFit.expand,
        children: [
          const Row(
            children: [
              Expanded(child: ColoredBox(color: _lightBg)),
              Expanded(child: ColoredBox(color: _darkBg)),
            ],
          ),
          Center(
            child: ShaderMask(
              blendMode: BlendMode.srcIn,
              shaderCallback: (r) => const LinearGradient(
                colors: [_lightInk, _lightInk, _darkInk, _darkInk],
                stops: [0, .5, .5, 1],
              ).createShader(r),
              child: const Text('Aa', style: aa),
            ),
          ),
        ],
      ),
    };
    return ExcludeSemantics(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(10),
        child: DecoratedBox(
          position: DecorationPosition.foreground,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(10),
            border: Border.all(width: .5, color: const Color(0x4D7F7F7F)),
          ),
          child: face,
        ),
      ),
    );
  }
}

/// The language's code over a greeting in it.
class _LangSwatch extends StatelessWidget {
  const _LangSwatch({
    required this.code,
    required this.hello,
    required this.selected,
  });
  final String code, hello;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final t = context.tokens;
    final fg = selected ? t.accentInk : t.ink;
    return ExcludeSemantics(
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: selected ? t.accentBg : t.ink.withValues(alpha: .05),
          borderRadius: BorderRadius.circular(10),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              code,
              style: TextStyle(
                fontSize: 17,
                fontWeight: FontWeight.w700,
                letterSpacing: .34,
                height: 1.1,
                color: fg,
              ),
            ),
            Text(
              hello,
              style: TextStyle(
                fontSize: 11,
                fontWeight: FontWeight.w500,
                color: selected ? t.accentInk : t.ink3,
              ),
            ),
          ],
        ),
      ),
    );
  }
}
