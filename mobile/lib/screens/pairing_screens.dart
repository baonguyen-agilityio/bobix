// ignore_for_file: sort_child_properties_last
// (a page lists its content before its actions, as it reads on screen)
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../config/flavor.dart';
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
    return Scaffold(
      backgroundColor: context.tokens.bg,
      body: SafeArea(
        child: switch (p.step) {
          PairStep.welcome => const _Welcome(),
          PairStep.scan => _Scan(denied: p.cameraDenied),
          PairStep.relay => _RelayNotice(host: p.relayHost ?? ''),
          PairStep.details => const _Details(),
          PairStep.waiting => const _Waiting(),
          PairStep.done => _Done(p),
          PairStep.failed => _Failed(p),
        },
      ),
    );
  }
}

class _Page extends ConsumerWidget {
  const _Page({
    required this.title,
    required this.children,
    this.actions = const [],
    this.icon,
  });

  final String title;
  final List<Widget> children;
  final List<Widget> actions;
  final IconData? icon;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.tokens;
    return ListView(
      padding: const EdgeInsets.fromLTRB(20, 24, 20, 32),
      children: [
        if (icon != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 12),
            child: Icon(icon, size: 40, color: t.accent),
          ),
        Semantics(
          header: true,
          child: Text(
            title,
            style: TextStyle(
              fontSize: 28,
              fontWeight: FontWeight.w700,
              letterSpacing: -.5,
              color: t.ink,
            ),
          ),
        ),
        const SizedBox(height: 12),
        ...children,
        const SizedBox(height: 20),
        for (final a in actions)
          Padding(padding: const EdgeInsets.only(bottom: 10), child: a),
      ],
    );
  }
}

Widget _lead(BuildContext context, String text) => Padding(
  padding: const EdgeInsets.only(bottom: 12),
  child: Text(
    text,
    style: TextStyle(fontSize: 16, height: 1.4, color: context.tokens.ink2),
  ),
);

class _Welcome extends ConsumerWidget {
  const _Welcome();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return _Page(
      icon: Icons.phone_iphone,
      title: s('phone.pair.welcome.t'),
      children: [_lead(context, s('phone.pair.welcome.d'))],
      actions: [
        PillButton(
          label: s('phone.pair.start'),
          primary: true,
          icon: Icons.qr_code_scanner,
          onPressed: ref.read(pairingControllerProvider.notifier).toScan,
        ),
      ],
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
    return _Page(
      title: s('phone.pair.scan.t'),
      children: [
        _lead(context, s('phone.pair.scan.d')),
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
            borderRadius: BorderRadius.circular(20),
            child: AspectRatio(
              aspectRatio: 1,
              child: ColoredBox(
                color: t.surface3,
                child: ref.watch(scannerBuilderProvider)(
                  context,
                  onCode: c.scanned,
                  onDenied: c.cameraDenied,
                ),
              ),
            ),
          ),
        // A phone in a simulator has no camera. The development flavor takes the code typed in; the other never does.
        if (ref.watch(flavorProvider) == Flavor.dev) ...[
          const SizedBox(height: 16),
          TextField(
            controller: _dev,
            decoration: InputDecoration(labelText: s('phone.pair.dev')),
            onSubmitted: c.scanned,
          ),
        ],
      ],
      actions: [PillButton(label: s('phone.pair.cancel'), onPressed: c.cancel)],
    );
  }
}

class _RelayNotice extends ConsumerWidget {
  const _RelayNotice({required this.host});
  final String host;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final c = ref.read(pairingControllerProvider.notifier);
    return _Page(
      title: s('phone.pair.relay', {'h': host}),
      children: [
        StatusBanner(
          kind: BannerKind.warn,
          icon: Icons.dns_outlined,
          title: host,
          body: s('phone.pair.relay.d'),
        ),
      ],
      actions: [
        PillButton(
          label: s('phone.pair.go'),
          primary: true,
          onPressed: c.relayAccepted,
        ),
        PillButton(label: s('phone.pair.cancel'), onPressed: c.cancel),
      ],
    );
  }
}

const _rightNames = ['answer', 'plan', 'question', 'start', 'stop', 'activity'];

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
    return _Page(
      title: s('phone.pair.name.t'),
      children: [
        TextField(
          controller: _name,
          maxLength: 64,
          decoration: const InputDecoration(border: OutlineInputBorder()),
          onChanged: (_) => setState(() {}),
        ),
        const SizedBox(height: 8),
        Text(
          s('phone.pair.rights.t'),
          style: TextStyle(
            fontSize: 18,
            fontWeight: FontWeight.w700,
            color: t.ink,
          ),
        ),
        _lead(context, s('phone.pair.rights.d')),
        Glass(
          // The tiles paint their ink on the nearest Material, which the glass must not hide.
          child: Material(
            type: MaterialType.transparency,
            child: Column(
              children: [
                for (final n in _rightNames)
                  SwitchListTile(
                    title: Text(s('phone.right.$n')),
                    subtitle: n == 'stop'
                        ? Text(s('phone.right.stop.d'))
                        : null,
                    value: _on[n]! && !(n == 'stop' && !_on['answer']!),
                    // Stop denies a held request, so it needs the right to answer too.
                    onChanged: n == 'stop' && !_on['answer']!
                        ? null
                        : (v) => setState(() => _on[n] = v),
                  ),
              ],
            ),
          ),
        ),
      ],
      actions: [
        PillButton(
          label: s('phone.pair.ask'),
          primary: true,
          onPressed: _name.text.trim().isEmpty
              ? null
              : () => ref
                    .read(pairingControllerProvider.notifier)
                    .submit(_name.text, _rights),
        ),
        PillButton(
          label: s('phone.pair.cancel'),
          onPressed: ref.read(pairingControllerProvider.notifier).cancel,
        ),
      ],
    );
  }
}

class _Waiting extends ConsumerWidget {
  const _Waiting();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    return _Page(
      title: s('phone.pair.wait.t'),
      children: [
        _lead(context, s('phone.pair.wait.d')),
        const Center(child: CircularProgressIndicator()),
      ],
      actions: [
        PillButton(
          label: s('phone.pair.cancel'),
          onPressed: ref.read(pairingControllerProvider.notifier).cancel,
        ),
      ],
    );
  }
}

class _Done extends ConsumerWidget {
  const _Done(this.p);
  final PairingState p;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final s = ref.watch(stringsProvider);
    final t = context.tokens;
    final c = p.computer!;
    final granted = rightsToJson(c.rights);
    final asked = rightsToJson(p.asked!);
    final names = _rightNames.where((n) => granted[n]!).toList();
    final fewer = _rightNames.any((n) => asked[n]! && !granted[n]!);
    return _Page(
      icon: Icons.check_circle_outline,
      title: s('phone.pair.done.t', {'c': c.name}),
      children: [
        if (names.isEmpty)
          _lead(context, s('phone.pair.done.none'))
        else ...[
          _lead(context, s('phone.pair.done.d')),
          for (final n in names)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Row(
                children: [
                  Icon(Icons.check, size: 18, color: t.ok),
                  const SizedBox(width: 8),
                  Expanded(child: Text(s('phone.right.$n'))),
                ],
              ),
            ),
        ],
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
      actions: [
        PillButton(
          label: s('phone.pair.open'),
          primary: true,
          onPressed: ref.read(pairingControllerProvider.notifier).finish,
        ),
      ],
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
      PairFailure.noStrongBiometrics => (
        s('phone.pair.e.bio'),
        s('phone.pair.e.bio.d'),
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
    return _Page(
      icon: Icons.error_outline,
      title: title,
      children: [_lead(context, body)],
      actions: [
        PillButton(
          label: s('phone.pair.again'),
          primary: true,
          onPressed: ref.read(pairingControllerProvider.notifier).restart,
        ),
        PillButton(
          label: s('phone.pair.cancel'),
          onPressed: ref.read(pairingControllerProvider.notifier).cancel,
        ),
      ],
    );
  }
}
