// ignore_for_file: sort_child_properties_last (the answers follow the content here as on screen)
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../data/phone_data.dart';
import '../l10n/l10n.dart';
import '../theme/tokens.dart';
import '../widgets/command_block.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/status_banner.dart';
import '../widgets/who_card.dart';

/// A held permission request: who asks, what it wants to run, and the answers. Sending an answer comes with the
/// connection; until then the buttons are drawn disabled.
class RequestScreen extends ConsumerStatefulWidget {
  const RequestScreen({super.key, required this.sessionId});

  final String sessionId;

  @override
  ConsumerState<RequestScreen> createState() => _RequestScreenState();
}

class _RequestScreenState extends ConsumerState<RequestScreen> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final request = ref.watch(requestProvider(widget.sessionId));
    final session = ref.watch(sessionProvider(widget.sessionId));
    final home = ref.watch(homeProvider).value;
    final computer = home?.computer ?? '';
    final title = s('phone.rq.title');
    final r = request.value, sess = session.value?.session;
    if (r == null || sess == null) {
      return DetailScaffold.loading(title: title);
    }

    final done = sess.answeredAtComputer;
    final locked = r.isLong && !_open;
    final care = r.destructive || r.hiddenCharacters;
    final note = locked
        ? s('phone.rq.openfirst')
        : '${s('phone.rq.unlock', {'os': 'Face ID'})} · ${s('phone.rq.waits')}';
    return DetailScaffold(
      title: title,
      children: [
        WhoCard(
          session: sess,
          subtitle: '${sess.project} · ${s('phone.rq.on', {'c': computer})}',
          badges: done
              ? [Pill(s('phone.rq.pc'), tone: PillTone.ok)]
              : [
                  Pill(
                    s('phone.home.waited', {
                      'n': s('phone.min', {'n': sess.waitingMinutes}),
                    }),
                    tone: PillTone.accent,
                  ),
                  Pill(s('phone.rq.exp', {'n': r.expiresMinutes})),
                  if (sess.handledAtComputer)
                    Pill(s('phone.hc.short'), tone: PillTone.info),
                ],
        ),
        if (sess.handledAtComputer)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: StatusBanner(
              kind: BannerKind.info,
              icon: Icons.person_outline,
              title: s('phone.rq.hc'),
              body: s('phone.rq.hc.d', {'c': computer}),
            ),
          ),
        if (done)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: StatusBanner(
              kind: BannerKind.info,
              icon: Icons.computer,
              title: s('phone.rq.pc'),
              body: s('phone.rq.pc.d', {'c': computer}),
            ),
          ),
        Padding(
          padding: const EdgeInsets.fromLTRB(0, 14, 0, 10),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.ask.h', {'tool': r.tool}),
              style: TextStyle(
                fontSize: 20,
                fontWeight: FontWeight.w700,
                color: t.ink,
                height: 1.25,
              ),
            ),
          ),
        ),
        if (r.destructive)
          WarningBand(
            icon: Icons.delete_outline,
            title: s('phone.rq.destr'),
            body: s('phone.rq.destr.d'),
          ),
        if (r.hiddenCharacters)
          WarningBand(
            icon: Icons.warning_amber_rounded,
            title: s('phone.rq.hid'),
            body: s('phone.rq.hid.d'),
          ),
        CommandBlock(
          r.command,
          marks: r.dangerous,
          clamped: locked,
          danger: care,
        ),
        if (r.isLong)
          Padding(
            padding: const EdgeInsets.only(top: 8),
            child: PillButton(
              compact: true,
              icon: _open ? Icons.expand_less : Icons.expand_more,
              label: _open
                  ? s('phone.rq.less')
                  : s('phone.rq.full', {
                      'n': _grouped(r.command.length, s.lang),
                    }),
              onPressed: () => setState(() => _open = !_open),
            ),
          ),
        Padding(
          padding: const EdgeInsets.only(top: 12),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                s('phone.rq.cwd'),
                style: TextStyle(fontSize: 13, color: t.ink3),
              ),
              const SizedBox(width: 14),
              Expanded(
                child: Text(
                  r.folder,
                  style: TextStyle(
                    fontFamily: 'Menlo',
                    fontFamilyFallback: const ['monospace'],
                    fontSize: 13,
                    color: t.ink,
                  ),
                ),
              ),
            ],
          ),
        ),
        Padding(
          padding: const EdgeInsets.only(top: 14),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Icon(Icons.shield_outlined, size: 16, color: t.ink3),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  s('phone.rq.never'),
                  style: TextStyle(fontSize: 12, height: 1.35, color: t.ink3),
                ),
              ),
            ],
          ),
        ),
      ],
      actions: done
          ? SizedBox(
              width: double.infinity,
              child: PillButton(
                label: s('phone.a.home'),
                onPressed: () => Navigator.of(context).maybePop(),
              ),
            )
          : Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (care) ...[
                  // The safe answer comes first and is the filled one.
                  SizedBox(
                    width: double.infinity,
                    child: PillButton(
                      primary: true,
                      icon: Icons.lock_outline,
                      label: s('phone.ask.deny'),
                    ),
                  ),
                  const SizedBox(height: 8),
                  SizedBox(
                    width: double.infinity,
                    child: PillButton(
                      danger: true,
                      icon: Icons.lock_outline,
                      label: s('phone.rq.allow.d'),
                    ),
                  ),
                ] else
                  Row(
                    children: [
                      Expanded(
                        flex: 4,
                        child: PillButton(
                          icon: Icons.lock_outline,
                          label: s('phone.ask.deny'),
                        ),
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        flex: 5,
                        child: PillButton(
                          primary: true,
                          icon: Icons.lock_outline,
                          label: s('phone.ask.allow'),
                        ),
                      ),
                    ],
                  ),
                FineNote(note),
              ],
            ),
    );
  }
}

String _grouped(int n, String lang) {
  final sep = lang == 'vi' ? '.' : ',';
  return n.toString().replaceAllMapped(
    RegExp(r'\B(?=(\d{3})+(?!\d))'),
    (_) => sep,
  );
}
