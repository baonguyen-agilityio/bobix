// Writes mobile/test/fixtures/creature-fixtures.json: a SHA-1 of the SVG that src/ui/creature.js draws for every design,
// colour, state, pose and frame, which mobile/test/creature_test.dart compares the Dart port with.
//   node test/creature-fixtures.js          write the file
//   node test/creature-fixtures.js --check  exit 1 when the file is stale (npm test runs this)
// Gradient ids (bx1, bx2 ...) count up per call, so both sides replace them by `bx` before hashing.
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const vm = require('vm');

const OUT = path.join(__dirname, '../mobile/test/fixtures/creature-fixtures.json');
const POSES = { working: ['typing', 'walkR', 'walkL'], waiting: ['waiting'], idle: ['asleep'], failed: ['failed'], error: ['failed'], done: ['wave', 'jump'] };

function build() {
  const ctx = {};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../src/ui/creature.js'), 'utf8'), ctx);
  const get = name => vm.runInContext(name, ctx);
  const [designs, loops, colors] = [get('DESIGNS'), get('LOOPS'), get('PET_COLORS')];
  const out = {};
  for (const design of designs) for (let ci = 0; ci < colors.length; ci++) for (const [st, poses] of Object.entries(POSES)) for (const ps of poses) for (let f = 0; f < loops[ps][0]; f++) {
    const svg = ctx.flatSVG(design, ci, st, [ps, f], true).replace(/bx\d+/g, 'bx');
    out[[design, ci, st, ps, f].join('/')] = crypto.createHash('sha1').update(svg).digest('hex');
  }
  return '{\n' + Object.entries(out).map(([k, v]) => `  "${k}": "${v}"`).join(',\n') + '\n}\n';
}

if (require.main === module) {
  const text = build();
  if (process.argv.includes('--check')) {
    if (!fs.existsSync(OUT) || fs.readFileSync(OUT, 'utf8') !== text) { console.error('creature-fixtures.json is out of date: run `node test/creature-fixtures.js`'); process.exit(1); }
  } else fs.writeFileSync(OUT, text);
}
module.exports = { build, OUT };
