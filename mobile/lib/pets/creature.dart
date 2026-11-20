// The flat pets of the desktop app (src/ui/creature.js) as SVG strings. The output must stay character-for-character
// equal to creature.js: test/creature-fixtures.js writes a hash of the desktop's SVG for every design, colour, state,
// pose and frame, and test/creature_test.dart compares this port with them. Change both files together.
import 'dart:math' as math;

const designs = ['default', 'nota', 'tty', 'miso', 'ship'];

/// The pet states a session shows: `error` is drawn as `failed`; `idle` is asleep, without a badge.
const petStates = ['working', 'waiting', 'idle', 'done', 'failed', 'error'];

int hashStr(String s) {
  var h = 2166136261;
  for (final r in s.runes) {
    // creature.js reads charCodeAt(0) of each code point: the high surrogate for one above U+FFFF.
    final unit = r > 0xFFFF ? 0xD800 + ((r - 0x10000) >> 10) : r;
    h = ((h ^ unit) * 16777619) & 0xFFFFFFFF;
  }
  return h;
}

/// The design of a project (its shape); an unknown design name draws the default pet.
String designOf(String id) => designs[hashStr(id) % designs.length];

// A number as JavaScript writes it into a template string.
String _s(num v) => v == v.truncate() ? v.truncate().toString() : v.toString();
// creature.js: n1 = v => +v.toFixed(2)
String _n(num v) => _s(double.parse(v.toStringAsFixed(2)));

const _face = '#1f2230', _cheek = '#f6a9bd';
const _statusFill = {
  'working': '#5b8def',
  'waiting': '#ff7a4d',
  'done': '#34c38f',
  'failed': '#e5484d',
};
const _status = {
  'working': 'working',
  'waiting': 'waiting',
  'done': 'done',
  'failed': 'failed',
  'error': 'failed',
};

/// The ms a frame lasts and the frames of each pose loop.
const loops = {
  'walkR': (4, 150),
  'walkL': (4, 150),
  'typing': (4, 150),
  'waiting': (4, 220),
  'wave': (4, 150),
  'jump': (4, 150),
  'failed': (4, 300),
  'asleep': (2, 1200),
};

/// The poses a state shows (the pose a pet in that state is drawn with while it plays).
const posesOf = {
  'working': ['typing', 'walkR', 'walkL'],
  'waiting': ['waiting'],
  'idle': ['asleep'],
  'failed': ['failed'],
  'error': ['failed'],
  'done': ['wave', 'jump'],
};

typedef Pose = (String, int);

/// The pose a pet in [state] shows [t] ms into its loop; [walk] is 'R' or 'L' while it walks that way.
Pose poseAt(String state, [int t = 0, String walk = '']) {
  if (state == 'done') {
    final (wn, wms) = loops['wave']!;
    final k = t % (wn * wms * 2);
    return k < wn * wms ? ('wave', k ~/ wms) : ('jump', (k - wn * wms) ~/ wms);
  }
  final ps = state == 'working' && walk.isNotEmpty
      ? 'walk$walk'
      : const {
              'waiting': 'waiting',
              'idle': 'asleep',
              'failed': 'failed',
              'error': 'failed',
            }[state] ??
            'typing';
  final (n, ms) = loops[ps]!;
  return (ps, (t ~/ ms) % n);
}

class _Pose {
  double dy = 0, rot = 0, sx = 1, sy = 1, lift = 0, shadow = 1, look = 0;
  List<double> feet = [0, 0];
  List<List<double>> hands = [
    [6.5, 21],
    [29.5, 21],
  ];
  String eyes = 'open', mouth = 'smile';
  bool cheeks = false;
  int blink = 1;
}

_Pose _flatPose(String ps, int f) {
  final o = _Pose();
  switch (ps) {
    case 'walkR':
      o.dy = f % 2 == 1 ? -1 : 0;
      o.rot = const [-3.0, 0.0, 3.0, 0.0][f];
      o.feet = const [
        [1.6, 0.0],
        [0.0, 0.0],
        [0.0, 1.6],
        [0.0, 0.0],
      ][f];
      o.hands = f < 2
          ? [
              [6.5, 19],
              [29.5, 23],
            ]
          : [
              [6.5, 23],
              [29.5, 19],
            ];
      o.look = .8;
      o.mouth = 'small';
      o.blink = f % 2;
    case 'walkL':
      final r = _flatPose('walkR', f);
      r.rot = -r.rot;
      r.look = -r.look;
      r.feet = [r.feet[1], r.feet[0]];
      r.hands = [
        [36 - r.hands[1][0], r.hands[1][1]],
        [36 - r.hands[0][0], r.hands[0][1]],
      ];
      return r;
    case 'typing':
      o.eyes = 'down';
      o.mouth = 'small';
      o.hands = f % 2 == 1
          ? [
              [13, 25],
              [23, 26.6],
            ]
          : [
              [13, 26.6],
              [23, 25],
            ];
      o.blink = f % 2;
    case 'waiting':
      o.eyes = 'look';
      o.mouth = 'o';
      o.hands = [
        [5, 22],
        [31, 22],
      ];
      o.feet = [0, f % 2 == 1 ? 1.8 : 0];
      o.sy = f % 2 == 1 ? 1.02 : 1;
    case 'wave':
      o.eyes = 'happy';
      o.mouth = 'open';
      o.cheeks = true;
      o.hands = [
        [6.5, 22],
        f % 2 == 1 ? [32.5, 9.5] : [31, 5.5],
      ];
      o.rot = f % 2 == 1 ? 2 : -1;
    case 'jump':
      o.eyes = 'happy';
      o.mouth = 'open';
      o.cheeks = true;
      o.lift = const [0.0, 4.0, 6.0, 2.0][f];
      o.sy = f == 0
          ? .92
          : f == 2
          ? 1.04
          : 1;
      o.sx = f == 0 ? 1.06 : 1;
      o.hands = f != 0
          ? [
              [5.5, 7],
              [30.5, 7],
            ]
          : [
              [6.5, 23],
              [29.5, 23],
            ];
      o.shadow = const [1.0, .7, .55, .85][f];
      o.feet = f != 0 ? [1, 1] : [0, 0];
    case 'failed':
      o.dy = 2;
      o.rot = 5;
      o.sy = .95;
      o.eyes = 'sad';
      o.mouth = 'frown';
      o.hands = [
        [8, 28.5],
        [28, 28.5],
      ];
    case 'asleep':
      o.dy = f != 0 ? 1.6 : 1;
      o.sy = f != 0 ? .97 : 1;
      o.eyes = 'closed';
      o.mouth = 'none';
      o.hands = [
        [8.5, 26.5],
        [27.5, 26.5],
      ];
  }
  return o;
}

String _faceSvg(double cx, double ey, double gap, _Pose o) {
  const ink = _face;
  final s = StringBuffer();
  for (final x0 in [cx - gap / 2, cx + gap / 2]) {
    final x = _n(x0 + o.look), d = x0 < cx ? 1 : -1;
    switch (o.eyes) {
      case 'open':
        s.write('<circle cx="$x" cy="${_s(ey)}" r="1.5" fill="$ink"/>');
      case 'look':
        s.write(
          '<circle cx="$x" cy="${_s(ey)}" r="2.05" fill="$ink"/><circle cx="${_n(double.parse(x) - .65)}" cy="${_n(ey - .75)}" r=".7" fill="#fff"/>',
        );
      case 'down':
        s.write(
          '<ellipse cx="$x" cy="${_s(ey + 1)}" rx="1.5" ry=".8" fill="$ink"/>',
        );
      case 'happy':
        s.write(
          '<path d="M${_n(double.parse(x) - 1.7)} ${_n(ey + .9)}Q$x ${_n(ey - 1.3)} ${_n(double.parse(x) + 1.7)} ${_n(ey + .9)}" fill="none" stroke="$ink" stroke-width="1.25" stroke-linecap="round"/>',
        );
      case 'sad':
        s.write(
          '<circle cx="$x" cy="${_s(ey + 1)}" r="1.2" fill="$ink"/><path d="M${_n(double.parse(x) - 1.6 * d)} ${_n(ey - .8)}L${_n(double.parse(x) + 1.3 * d)} ${_n(ey - 2)}" stroke="$ink" stroke-width="1" stroke-linecap="round"/>',
        );
      case 'closed':
        s.write(
          '<path d="M${_n(double.parse(x) - 1.7)} ${_n(ey + .5)}Q$x ${_n(ey + 1.8)} ${_n(double.parse(x) + 1.7)} ${_n(ey + .5)}" fill="none" stroke="$ink" stroke-width="1.15" stroke-linecap="round"/>',
        );
    }
  }
  if (o.cheeks) {
    s.write(
      '<ellipse cx="${_n(cx - gap / 2 - 2.4)}" cy="${_n(ey + 2.7)}" rx="1.5" ry=".85" fill="$_cheek"/><ellipse cx="${_n(cx + gap / 2 + 2.4)}" cy="${_n(ey + 2.7)}" rx="1.5" ry=".85" fill="$_cheek"/>',
    );
  }
  final mx = _n(cx + o.look), mxn = double.parse(mx), my = ey + 4;
  const st =
      'fill="none" stroke="$ink" stroke-width="1.1" stroke-linecap="round"';
  switch (o.mouth) {
    case 'smile':
      s.write(
        '<path d="M${_n(mxn - 1.6)} ${_s(my)}Q$mx ${_n(my + 1.6)} ${_n(mxn + 1.6)} ${_s(my)}" $st/>',
      );
    case 'small':
      s.write(
        '<path d="M${_n(mxn - .9)} ${_n(my + .3)}H${_n(mxn + .9)}" $st/>',
      );
    case 'o':
      s.write(
        '<ellipse cx="$mx" cy="${_n(my + .4)}" rx=".95" ry="1.15" fill="$ink"/>',
      );
    case 'open':
      s.write(
        '<path d="M${_n(mxn - 2)} ${_n(my - .2)}Q$mx ${_n(my + 3.2)} ${_n(mxn + 2)} ${_n(my - .2)}Z" fill="$ink"/>',
      );
    case 'frown':
      s.write(
        '<path d="M${_n(mxn - 1.6)} ${_n(my + 1.1)}Q$mx ${_n(my - .5)} ${_n(mxn + 1.6)} ${_n(my + 1.1)}" $st/>',
      );
  }
  return s.toString();
}

String _ttyFace(_Pose o, String glow) {
  final s = StringBuffer();
  for (final x0 in [13.2, 22.8]) {
    final x = _n(x0 - 1 + o.look), xn = double.parse(x);
    switch (o.eyes) {
      case 'open':
        s.write(
          '<rect x="$x" y="11" width="2" height="3" rx=".4" fill="$glow"/>',
        );
      case 'look':
        s.write(
          '<rect x="${_n(xn - .3)}" y="10.4" width="2.6" height="3.8" rx=".5" fill="$glow"/>',
        );
      case 'down':
        s.write(
          '<rect x="$x" y="13" width="2.2" height="1.4" rx=".4" fill="$glow"/>',
        );
      case 'happy':
        s.write(
          '<path d="M${_n(xn - .7)} 13.4L${_n(xn + 1)} 11.6L${_n(xn + 2.7)} 13.4" fill="none" stroke="$glow" stroke-width="1" stroke-linecap="round" stroke-linejoin="round"/>',
        );
      case 'sad':
        s.write(
          '<rect x="$x" y="12.2" width="2" height="2" rx=".4" fill="$glow"/><path d="M${_n(x0 < 18 ? xn - .4 : xn + 2.4)} 10.8L${_n(x0 < 18 ? xn + 2 : xn)} 10" stroke="$glow" stroke-width=".9" stroke-linecap="round"/>',
        );
      case 'closed':
        s.write(
          '<rect x="${_n(xn - .5)}" y="13.4" width="3" height=".9" rx=".45" fill="$glow"/>',
        );
    }
  }
  final tall = o.mouth == 'open' || o.mouth == 'o';
  if (o.blink != 0 || tall) {
    s.write(
      '<rect x="${_n(16.5 + o.look)}" y="16.6" width="3" height="${tall ? 2 : 1.1}" rx=".4" fill="$glow"/>',
    );
  }
  return s.toString();
}

class _Colors {
  const _Colors(this.hi, this.lo);
  final String hi, lo;
}

class _Design {
  const _Design({
    required this.feet,
    required this.hand,
    required this.grad,
    required this.body,
    required this.face,
    this.across = false,
  });
  final List<double> feet;
  final String Function(_Colors) hand;
  final (String, String) Function(_Colors) grad;
  final String Function(String g, _Colors c) body;
  final String Function(_Pose o, String st) face;
  final bool across;
}

(String, String) _hiLo(_Colors c) => (c.hi, c.lo);
String _lo(_Colors c) => c.lo;

final _flat = <String, _Design>{
  'default': _Design(
    feet: [10.3, 20.1],
    hand: _lo,
    grad: _hiLo,
    body: (g, c) =>
        '<circle cx="11.5" cy="8.6" r="3.3" fill="$g"/><circle cx="24.5" cy="8.6" r="3.3" fill="$g"/><rect x="6.5" y="7" width="23" height="22" rx="8.5" fill="$g"/>',
    face: (o, st) => _faceSvg(18, 16, 7, o),
  ),
  'nota': _Design(
    feet: [10.8, 19.6],
    hand: _lo,
    grad: _hiLo,
    body: (g, c) =>
        '<rect x="8" y="4" width="20" height="26" rx="5" fill="$g"/><rect x="24.5" y="5.6" width="2.3" height="22.8" rx="1.15" fill="#fbf6ea"/>'
        '${[9, 13.5, 18, 22.5].map((y) => '<circle cx="8.7" cy="${_s(y)}" r="1.05" fill="#fbf6ea"/>').join()}'
        '<rect x="12" y="21.4" width="9.6" height="4" rx="1.2" fill="#fbf6ea" opacity=".92"/>',
    face: (o, st) => _faceSvg(16.8, 12.4, 7, o),
  ),
  'tty': _Design(
    feet: [10.6, 19.8],
    hand: _lo,
    grad: _hiLo,
    body: (g, c) =>
        '<rect x="15" y="23.5" width="6" height="5" rx="1" fill="${c.lo}"/><rect x="5.5" y="5" width="25" height="20" rx="5" fill="$g"/><rect x="8.5" y="7.8" width="19" height="13" rx="3" fill="#1e2a33"/>',
    face: (o, st) => _ttyFace(
      o,
      st == 'failed' ? '#ff6b6b' : (st == 'idle' ? '#3f7a50' : '#7df29b'),
    ),
  ),
  'miso': _Design(
    feet: [10.3, 20.1],
    hand: _lo,
    grad: _hiLo,
    body: (g, c) =>
        '<path d="M28 25.5Q34.5 23.5 32.2 15.8" fill="none" stroke="${c.lo}" stroke-width="2.6" stroke-linecap="round"/>'
        '<path d="M9.2 11L10.6 3L16 8Z" fill="$g" stroke="${c.hi}" stroke-width="2" stroke-linejoin="round"/><path d="M26.8 11L25.4 3L20 8Z" fill="$g" stroke="${c.hi}" stroke-width="2" stroke-linejoin="round"/>'
        '<path d="M11.2 7.6L11.7 5.2L13.6 7Z" fill="$_cheek"/><path d="M24.8 7.6L24.3 5.2L22.4 7Z" fill="$_cheek"/>'
        '<rect x="6.5" y="6.5" width="23" height="22.5" rx="10" fill="$g"/><ellipse cx="18" cy="20.2" rx="4.4" ry="3.1" fill="#f6f3fb"/>'
        '<path d="M3.4 17.6H7.6M3.8 20.2H7.6M28.4 17.6H32.6M28.4 20.2H32.2" stroke="$_face" stroke-opacity=".35" stroke-width=".7" stroke-linecap="round"/><path d="M17 17.4h2l-1 1.1z" fill="#e07a9a"/>',
    face: (o, st) => _faceSvg(18, 14.2, 7, o),
  ),
  'ship': _Design(
    feet: [11.8, 18.6],
    hand: (_) => '#cdd5e2',
    grad: (_) => ('#f6f8fc', '#cdd5e2'),
    across: true,
    body: (g, c) =>
        '<path d="M11.2 19.5L6.6 26.6Q6.3 28.6 8.3 28.4L11.6 27Z" fill="${c.lo}"/><path d="M24.8 19.5L29.4 26.6Q29.7 28.6 27.7 28.4L24.4 27Z" fill="${c.lo}"/>'
        '<rect x="10.5" y="5.5" width="15" height="23.5" rx="7.5" fill="$g"/><path d="M10.7 11.2Q18 -2.2 25.3 11.2Z" fill="${c.lo}"/>'
        '<circle cx="18" cy="13.6" r="2.9" fill="#2d4a7a" stroke="#cdd5e2" stroke-width="1"/><circle cx="17.1" cy="12.7" r=".85" fill="#9fd8ff"/>',
    face: (o, st) => _faceSvg(18, 20.2, 5.4, o),
  ),
};

String _badge(String? st) {
  const x = 29.6, y = 6.4;
  const w =
      'stroke="#fff" stroke-width="1.3" stroke-linecap="round" fill="none"';
  final glyph = switch (st) {
    'working' =>
      '<circle cx="${_n(x - 1.8)}" cy="${_s(y)}" r=".65" fill="#fff"/><circle cx="${_s(x)}" cy="${_s(y)}" r=".65" fill="#fff"/><circle cx="${_n(x + 1.8)}" cy="${_s(y)}" r=".65" fill="#fff"/>',
    'waiting' =>
      '<path d="M${_s(x)} ${_n(y - 2.4)}V${_n(y + .5)}" $w/><circle cx="${_s(x)}" cy="${_n(y + 2.1)}" r=".72" fill="#fff"/>',
    'done' =>
      '<path d="M${_n(x - 2)} ${_n(y + .1)}L${_n(x - .5)} ${_n(y + 1.6)}L${_n(x + 2.1)} ${_n(y - 1.5)}" $w/>',
    'failed' =>
      '<path d="M${_n(x - 1.6)} ${_n(y - 1.6)}L${_n(x + 1.6)} ${_n(y + 1.6)}M${_n(x + 1.6)} ${_n(y - 1.6)}L${_n(x - 1.6)} ${_n(y + 1.6)}" $w/>',
    _ => null,
  };
  return glyph == null
      ? ''
      : '<circle cx="${_s(x)}" cy="${_s(y)}" r="4.7" fill="${_statusFill[st]}" stroke="#fff" stroke-width="1.2"/>$glyph';
}

String _statusMarks(String? status, int f) => status == 'waiting'
    ? '<rect class="ring" x="1.6" y="-.6" width="32.8" height="34" rx="10.5" fill="${_statusFill['waiting']}" fill-opacity=".14" stroke="${_statusFill['waiting']}" stroke-width="1.7" opacity="${_s(const [1, .82, .6, .82][f % 4])}"/>'
    : '';

String _hex32(double L, double C, double h) {
  final a = C * math.cos(h * math.pi / 180),
      b = C * math.sin(h * math.pi / 180);
  final l = math.pow(L + .3963377774 * a + .2158037573 * b, 3),
      m = math.pow(L - .1055613458 * a - .0638541728 * b, 3),
      s = math.pow(L - .0894841775 * a - 1.291485548 * b, 3);
  int g(num x) =>
      (255 *
              (x <= .0031308 ? 12.92 * x : 1.055 * math.pow(x, 1 / 2.4) - .055)
                  .clamp(0, 1))
          .round();
  String two(int v) => v.toRadixString(16).padLeft(2, '0');
  return '#${two(g(4.0767416621 * l - 3.3077115913 * m + .2309699292 * s))}${two(g(-1.2684380046 * l + 2.6097574011 * m - .3413193965 * s))}${two(g(-.0041960863 * l - .7034186147 * m + 1.707614701 * s))}';
}

// The session colours: the design tokens sky, mint, lilac, peach, butter and rose (oklch L, C, h).
const _bodyTokens = [
  [.84, .075, 235.0],
  [.86, .085, 162.0],
  [.83, .075, 300.0],
  [.86, .075, 55.0],
  [.90, .095, 98.0],
  [.84, .075, 5.0],
];
final petColors = [
  for (final [L, C, h] in _bodyTokens)
    _Colors(_hex32(L, C, h), _hex32(L - .1, C + .03, h)),
];

/// The number of session colours; a colour index is taken modulo this.
int get petColorCount => petColors.length;

final _svgCache = <String, String>{};

/// The pet as SVG, every colour written out. An unknown [design] draws the default pet and a colour index outside the
/// list wraps round. [state] is one of [petStates]; [pose] is a (pose, frame) from [poseAt].
///
/// The same arguments return the same string, so flutter_svg's cache (keyed by the string) hits. The set of strings is
/// small: a few designs, colours, states and pose frames.
String flatSvg(
  String design,
  int colorIndex,
  String state,
  Pose pose, {
  bool withBadge = true,

  /// The coral ring behind a waiting pet; a pet that wears the halo instead is drawn without it.
  bool ring = true,
}) {
  final key =
      '${_flat.containsKey(design) ? design : 'default'}|${colorIndex % petColors.length}|$state|${pose.$1}|${pose.$2}|$withBadge|$ring';
  return _svgCache[key] ??= _flatSvg(
    design,
    colorIndex,
    state,
    pose,
    withBadge: withBadge,
    ring: ring,
  );
}

String _flatSvg(
  String design,
  int colorIndex,
  String state,
  Pose pose, {
  bool withBadge = true,

  /// The coral ring behind a waiting pet; a pet that wears the halo instead is drawn without it.
  bool ring = true,
}) {
  final p = _flat[design] ?? _flat['default']!;
  final c = petColors[colorIndex % petColors.length];
  final (ps, f) = pose;
  final o = _flatPose(ps, f);
  // An id only resolves inside its own SVG, so one fixed id is safe.
  const id = 'bx';
  final (g0, g1) = p.grad(c);
  final status = _status[state];
  final t =
      'translate(18 30) rotate(${_s(o.rot)}) scale(${_s(o.sx)} ${_s(o.sy)}) translate(-18 -30) translate(0 ${_s(o.dy)})';
  final hand = p.hand(c);
  final feet = [
    for (var i = 0; i < p.feet.length; i++)
      '<rect x="${_s(p.feet[i])}" y="${_n(28.4 - o.feet[i])}" width="5.6" height="3.2" rx="1.6" fill="$hand"/>',
  ].join();
  final hands = [
    for (final h in o.hands)
      '<circle cx="${_s(h[0])}" cy="${_s(h[1])}" r="2.3" fill="$hand"/>',
  ].join();
  final s = StringBuffer(
    '<defs><linearGradient id="$id" x1="0" y1="0" x2="${p.across ? 1 : 0}" y2="${p.across ? 0 : 1}"><stop offset="0" stop-color="$g0"/><stop offset="1" stop-color="$g1"/></linearGradient></defs>'
    '<ellipse cx="18" cy="31.7" rx="${_n(9 * o.shadow)}" ry="1.3" fill="#000" fill-opacity=".22"/>',
  );
  if (withBadge && ring) s.write(_statusMarks(status, f));
  s.write(
    '<g transform="translate(0 ${_s(-o.lift)})">$feet<g transform="$t">${p.body('url(#$id)', c)}${p.face(o, state)}$hands</g>${withBadge ? _badge(status) : ''}</g>',
  );
  return '<svg class="flat" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 36 36" width="100%" height="100%" overflow="visible" aria-hidden="true">$s</svg>';
}
