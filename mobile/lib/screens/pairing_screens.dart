// ignore_for_file: sort_child_properties_last
// (a page lists its content before its actions, as it reads on screen)
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../config/flavor.dart';
import '../features/actions/action_widgets.dart' show FinePrint, bioName;
import '../keys/platform_secure_keys.dart';
import '../l10n/l10n.dart';
import '../link/link_state.dart';
import '../link/pairing_controller.dart';
import '../link/pairing_service.dart';
import '../src/rust/api/messages.dart';
import '../theme/tokens.dart';
import '../widgets/detail_scaffold.dart';
import '../widgets/glass.dart';
import '../widgets/status_banner.dart';
import 'pair_chrome.dart';

/// Draws the camera and reports what it reads. A seam: the real one is [cameraScanner]; tests give a stand-in.
typedef ScannerBuilder = Widget Function(
  BuildContext context, {
  required void Function(String text) onCode,
  required void Function() onDenied,
});

Widget cameraScanner(
  BuildContext context, {
  required void Function(String text) onCode,
  required void Function() onDenied,
}) => MobileScanner(
  onDetect: (capture) {
    for (final b in capture.barcodes) {
      final v = b.rawValue;
      if (v != null) return onCode(v);
    }
  },
  errorBuilder: (context, error) {
    if (error.errorCode == MobileScannerErrorCode.permissionDenied) {
      WidgetsBinding.instance.addPostFrameCallback((_) => onDenied());
    }
    return Center(
      child: Icon(Icons.videocam_off_outlined, color: context.tokens.ink3),
    );
  },
);

final scannerBuilderProvider = Provider<ScannerBuilder>((ref) => cameraScanner);

/// The pairing flow: welcome, scan, name and rights, waiting for the computer, done. Every failure has its own screen.
class PairingFlow extends ConsumerWidget {
  const PairingFlow({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final p = ref.watch(pairingControllerProvider);
    return switch (p.step) {
      PairStep.welcome => const _Welcome(),
      PairStep.scan => _Scan(denied: p.cameraDenied),
      PairStep.relay => _RelayNotice(host: p.relayHost ?? ''),
      PairStep.details => const _Details(),
      PairStep.waiting => const _Waiting(),
      PairStep.done => _Done(p),
      PairStep.failed when p.failure == PairFailure.noStrongBiometrics =>
        const _NoBiometrics(),
      PairStep.failed => _Failed(p),
      PairStep.noBiometrics => const _NoBiometrics(),
    };
  }
}

class _Welcome extends ConsumerWidget {
  const _Welcome();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return PairPage(
      step: 1,
      children: [
        PairHero(
          icon: Icons.desktop_mac_outlined,
          iconSize: 36,
          tone: HeroTone.info,
          title: s('phone.pair.welcome.t'),
          text: s('phone.pair.welcome.d'),
        ),
        const SizedBox(height: 6),
        NumberedSteps([
          s('phone.pair.s1'),
          s('phone.pair.s2'),
          s('phone.pair.s3'),
        ]),
        const SizedBox(height: 18),
        ShieldNote(s('phone.pair.welcome.n')),
      ],
      actions: PillButton(
        label: s('phone.pair.start'),
        primary: true,
        icon: Icons.qr_code_scanner,
        onPressed: ref.read(pairingControllerProvider.notifier).toScan,
      ),
    );
  }
}

class _Scan extends ConsumerStatefulWidget {
  const _Scan({required this.denied});
  final bool denied;

  @override
  ConsumerState<_Scan> createState() => _ScanState();
}

class _ScanState extends ConsumerState<_Scan> {
  final _dev = TextEditingController();

  @override
  void dispose() {
    _dev.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      step: 2,
      onBack: c.cancel,
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.pair.scan.t'),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 20,
              fontWeight: FontWeight.w700,
              height: 1.25,
              letterSpacing: -.2,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 12),
        if (widget.denied)
          StatusBanner(
            kind: BannerKind.warn,
            icon: Icons.videocam_off_outlined,
            title: s('phone.pair.cam.no'),
            body: s('phone.pair.cam.no.d'),
            action: PillButton(
              compact: true,
              label: s('phone.pair.cam.settings'),
              onPressed: openAppSettings,
            ),
          )
        else
          ClipRRect(
            borderRadius: BorderRadius.circular(22),
            child: AspectRatio(
              aspectRatio: 1,
              child: ColoredBox(
                color: const Color(0xFF0F1114),
                child: Stack(
                  fit: StackFit.expand,
                  children: [
                    ref.watch(scannerBuilderProvider)(
                      context,
                      onCode: c.scanned,
                      onDenied: c.cameraDenied,
                    ),
                    const IgnorePointer(
                      child: CustomPaint(painter: _CornerBrackets()),
                    ),
                  ],
                ),
              ),
            ),
          ),
        const SizedBox(height: 12),
        Text(
          s('phone.pair.scan.d'),
          textAlign: TextAlign.center,
          style: TextStyle(fontSize: 14, height: 1.45, color: t.ink2),
        ),
        // A phone in a simulator has no camera. The development flavor takes the code typed in; the other never does.
        if (ref.watch(flavorProvider) == Flavor.dev) ...[
          const SizedBox(height: 16),
          Glass(
            radius: 12,
            padding: const EdgeInsets.symmetric(horizontal: 14),
            child: TextField(
              controller: _dev,
              decoration: InputDecoration(
                labelText: s('phone.pair.dev'),
                border: InputBorder.none,
              ),
              onSubmitted: c.scanned,
            ),
          ),
        ],
      ],
    );
  }
}

/// The four white corners of the scanning frame, 14 % in from each side of the panel.
class _CornerBrackets extends CustomPainter {
  const _CornerBrackets();

  @override
  void paint(Canvas canvas, Size size) {
    final r = Rect.fromLTWH(
      size.width * .14,
      size.height * .14,
      size.width * .72,
      size.height * .72,
    );
    final len = r.width * .28;
    final paint = Paint()
      ..color = Colors.white
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3
      ..strokeCap = StrokeCap.round;
    const radius = 18.0;
    final path = Path();
    for (final (cx, cy, dx, dy) in [
      (r.left, r.top, 1.0, 1.0),
      (r.right, r.top, -1.0, 1.0),
      (r.left, r.bottom, 1.0, -1.0),
      (r.right, r.bottom, -1.0, -1.0),
    ]) {
      path
        ..moveTo(cx + dx * len, cy)
        ..lineTo(cx + dx * radius, cy)
        ..quadraticBezierTo(cx, cy, cx, cy + dy * radius)
        ..lineTo(cx, cy + dy * len);
    }
    canvas.drawPath(path, paint);
  }

  @override
  bool shouldRepaint(_CornerBrackets old) => false;
}

class _RelayNotice extends ConsumerWidget {
  const _RelayNotice({required this.host});
  final String host;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      step: 2,
      onBack: c.cancel,
      children: [
        Semantics(
          header: true,
          child: Text(
            s('phone.pair.relay', {'h': host}),
            textAlign: TextAlign.center,
            style: TextStyle(
              fontSize: 20,
              fontWeight: FontWeight.w700,
              height: 1.25,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 12),
        StatusBanner(
          kind: BannerKind.warn,
          icon: Icons.dns_outlined,
          title: host,
          body: s('phone.pair.relay.d'),
        ),
      ],
      actions: PillButton(
        label: s('phone.pair.go'),
        primary: true,
        onPressed: c.relayAccepted,
      ),
    );
  }
}

/// The rights, in the order the screen lists them, each with the name its switch has in [rightsFromJson].
const _rightNames = ['answer', 'question', 'plan', 'start', 'activity', 'stop'];

class _Details extends ConsumerStatefulWidget {
  const _Details();

  @override
  ConsumerState<_Details> createState() => _DetailsState();
}

class _DetailsState extends ConsumerState<_Details> {
  late final TextEditingController _name;
  final _on = {for (final n in _rightNames) n: true};

  @override
  void initState() {
    super.initState();
    _name = TextEditingController(
      text: ref.read(stringsProvider)('phone.pair.name.ph'),
    );
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  // Stop denies a held request, so it only counts with the right to answer.
  PhoneRights get _rights =>
      rightsFromJson({..._on, 'stop': _on['stop']! && _on['answer']!});

  @override
  Widget build(BuildContext context) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      step: 3,
      onBack: c.cancel,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: Text(
            s('phone.pair.name.t'),
            style: TextStyle(
              fontSize: 13,
              fontWeight: FontWeight.w600,
              color: t.ink2,
            ),
          ),
        ),
        Glass(
          radius: 12,
          padding: const EdgeInsets.symmetric(horizontal: 14),
          child: TextField(
            controller: _name,
            maxLength: 64,
            style: TextStyle(fontSize: 16, color: t.ink),
            decoration: const InputDecoration(
              border: InputBorder.none,
              counterText: '',
              contentPadding: EdgeInsets.symmetric(vertical: 16),
            ),
            onChanged: (_) => setState(() {}),
          ),
        ),
        Padding(
          padding: const EdgeInsets.only(top: 20, bottom: 8),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.pair.rights.t'),
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w600,
                color: t.ink,
              ),
            ),
          ),
        ),
        Glass(
          radius: 14,
          // The tiles paint their ink on the nearest Material, which the glass must not hide.
          child: Material(
            type: MaterialType.transparency,
            child: Column(
              children: [
                for (final n in _rightNames) ...[
                  if (n != _rightNames.first)
                    Divider(height: .5, thickness: .5, color: t.line),
                  SwitchListTile(
                    visualDensity: VisualDensity.compact,
                    minVerticalPadding: 6,
                    contentPadding: const EdgeInsets.symmetric(horizontal: 14),
                    title: Text(
                      s('phone.pair.may.$n'),
                      style: TextStyle(
                        fontSize: 15,
                        fontWeight: FontWeight.w500,
                        color: t.ink,
                      ),
                    ),
                    subtitle: Text(
                      // Stop denies a held request, so it needs the right to answer too: say so while that is off.
                      n == 'stop' && !_on['answer']!
                          ? s('phone.right.stop.d')
                          : s('phone.perm.$n.d'),
                      style: TextStyle(fontSize: 12, color: t.ink3),
                    ),
                    value: _on[n]! && !(n == 'stop' && !_on['answer']!),
                    onChanged: n == 'stop' && !_on['answer']!
                        ? null
                        : (v) => setState(() => _on[n] = v),
                  ),
                ],
                Divider(height: .5, thickness: .5, color: t.line),
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 10,
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      FirstLineIcon(
                        icon: Icon(shieldCheckIcon, size: 22, color: t.ink2),
                        size: 22,
                        style: _neverTitle(t),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(s('phone.pair.never'), style: _neverTitle(t)),
                            Text(
                              s('phone.pair.never.d'),
                              style: TextStyle(fontSize: 12, color: t.ink3),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 12),
        FinePrint(s('phone.pair.name.n')),
      ],
      actions: PillButton(
        label: s('phone.pair.ask'),
        primary: true,
        onPressed: _name.text.trim().isEmpty
            ? null
            : () => c.submit(_name.text, _rights),
      ),
    );
  }
}

class _Waiting extends ConsumerWidget {
  const _Waiting();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      step: 3,
      children: [
        const SizedBox(height: 26),
        const Center(child: CircularProgressIndicator()),
        PairHero(
          icon: Icons.hourglass_empty,
          tone: HeroTone.info,
          title: s('phone.pair.wait.t'),
          text: s('phone.pair.wait.d'),
        ),
      ],
      actions: PillButton(label: s('phone.pair.cancel'), onPressed: c.cancel),
    );
  }
}

class _Done extends ConsumerWidget {
  const _Done(this.p);
  final PairingState p;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = p.computer!;
    final granted = rightsToJson(c.rights);
    final asked = rightsToJson(p.asked!);
    final names = _rightNames.where((n) => granted[n]!).toList();
    final fewer = _rightNames.any((n) => asked[n]! && !granted[n]!);
    return PairPage(
      children: [
        PairHero(
          icon: Icons.check,
          tone: HeroTone.ok,
          title: s('phone.pair.done.t', {'c': c.name}),
          text: s('phone.pair.done.d'),
        ),
        KeyValueCard([
          (s('phone.pair.done.k1'), c.phoneName),
          (
            s('phone.pair.done.k2'),
            names.isEmpty
                ? s('phone.pair.done.none')
                : names.map((n) => s('phone.pair.may.$n')).join(', '),
          ),
          (
            s('phone.pair.done.k3'),
            s('phone.pair.done.v3', {'os': bioName(s)}),
          ),
        ]),
        if (fewer)
          Padding(
            padding: const EdgeInsets.only(top: 12),
            child: StatusBanner(
              kind: BannerKind.info,
              icon: Icons.info_outline,
              title: s('phone.pair.done.less'),
            ),
          ),
      ],
      actions: PillButton(
        label: s('phone.pair.open'),
        primary: true,
        onPressed: ref.read(pairingControllerProvider.notifier).finish,
      ),
    );
  }
}

/// The phone cannot do strong biometrics, so pairing cannot go on: what to do about it. Reached from the welcome page
/// (the platform says so before anything is scanned) and from a pairing that found out late.
class _NoBiometrics extends ConsumerWidget {
  const _NoBiometrics();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      onBack: c.cancel,
      children: [
        PairHero(
          icon: Icons.fingerprint,
          iconSize: 36,
          tone: HeroTone.warn,
          title: s('phone.pair.nb.h'),
          text: s('phone.pair.nb.d'),
        ),
        Padding(
          padding: const EdgeInsets.only(top: 16, bottom: 8),
          child: Semantics(
            header: true,
            child: Text(
              s('phone.pair.nb.todo'),
              style: TextStyle(
                fontSize: 15,
                fontWeight: FontWeight.w600,
                color: t.ink,
              ),
            ),
          ),
        ),
        NumberedSteps([
          s('phone.pair.nb.s1'),
          s('phone.pair.nb.s2'),
          s('phone.pair.nb.s3'),
        ]),
      ],
      actions: PillButton(label: s('phone.a.back'), onPressed: c.cancel),
    );
  }
}

class _Failed extends ConsumerWidget {
  const _Failed(this.p);
  final PairingState p;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final the = s('phone.pair.thecomputer');
    final (String title, String body) = switch (p.failure!) {
      PairFailure.notBobix => (s('phone.pair.e.bad'), s('phone.pair.e.bad.d')),
      PairFailure.insecureRelay => (
        s('phone.pair.e.insecure'),
        s('phone.pair.e.insecure.d'),
      ),
      PairFailure.codeExpired => (
        s('phone.pair.e.expired'),
        s('phone.pair.e.expired.d'),
      ),
      PairFailure.codeUsed => (
        s('phone.pair.e.used'),
        s('phone.pair.e.used.d'),
      ),
      PairFailure.relayUnreachable => (
        s('phone.pair.e.relay'),
        s('phone.pair.e.relay.d'),
      ),
      PairFailure.computerSaidNo => (
        s('phone.pair.e.no'),
        s('phone.pair.e.no.d'),
      ),
      PairFailure.twoPhones => (s('phone.pair.e.two'), s('phone.pair.e.two.d')),
      // Shown by [_NoBiometrics]; the pair never gets here.
      PairFailure.noStrongBiometrics => ('', ''),
      PairFailure.biometryLockedOut => (
        s('phone.pair.e.lock'),
        s('phone.pair.e.lock.d'),
      ),
      PairFailure.alreadyPaired => (
        s('phone.pair.e.have'),
        s('phone.pair.e.have.d'),
      ),
      PairFailure.keyFailed => (s('phone.pair.e.key'), s('phone.pair.e.key.d')),
      PairFailure.updateNeeded when p.phoneMustUpdate => (
        s('phone.link.update.phone'),
        s('phone.link.update.phone.d'),
      ),
      PairFailure.updateNeeded => (
        s('phone.link.update.computer', {'c': the}),
        s('phone.link.update.computer.d'),
      ),
      PairFailure.inviteNeeded => (
        s('phone.link.invite', {'c': the}),
        s('phone.link.invite.d'),
      ),
      PairFailure.cancelled => ('', ''),
    };
    final c = ref.read(pairingControllerProvider.notifier);
    return PairPage(
      onBack: c.cancel,
      children: [
        PairHero(
          icon: Icons.error_outline,
          tone: HeroTone.err,
          title: title,
          text: body,
        ),
      ],
      actions: PillButton(
        label: s('phone.pair.again'),
        primary: true,
        onPressed: c.restart,
      ),
    );
  }
}

/// The title of the "No Always allow" row; the shield is centred on its first line.
TextStyle _neverTitle(BobixTokens t) => TextStyle(
  fontSize: 15,
  height: 1.3,
  fontWeight: FontWeight.w500,
  color: t.ink,
);
