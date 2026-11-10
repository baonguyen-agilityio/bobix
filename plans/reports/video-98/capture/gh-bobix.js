#!/usr/bin/env node
// gh-bobix.js: the real `gh`, read-only, seen through the bobix repo only (for filming; nothing is written to GitHub).
// Bobix asks gh for the issues assigned to you; nothing in bobix is assigned to anyone, and the account's assigned issues
// live in other repos that must stay out of the film. So "assigned to you" reads as "open in baonguyen-agilityio/bobix",
// and your open pull requests are kept to that repo. Every other call goes to gh unchanged.
const { execFileSync } = require('node:child_process');
const GH = '/opt/homebrew/bin/gh', REPO = 'baonguyen-agilityio/bobix';
const a = process.argv.slice(2);
const run = args => { try { return execFileSync(GH, args, { encoding: 'utf8', maxBuffer: 64 << 20 }); } catch (e) { process.stderr.write(e.stderr || ''); process.exit(e.status || 1); } };
const i = a.findIndex(x => /^issues\?filter=assigned/.test(x));
if (a[0] === 'api' && i > 0) {
  a[i] = a[i].replace(/^issues\?filter=assigned&/, `repos/${REPO}/issues?`);
  process.stdout.write(run(a));
} else if (a[0] === 'api' && a.includes('graphql') && a.some(x => x.includes('viewer{pullRequests'))) {
  const v = JSON.parse(run(a)), p = v.data.viewer.pullRequests;
  p.nodes = p.nodes.filter(n => n.repository.nameWithOwner === REPO);
  process.stdout.write(JSON.stringify(v));
} else {
  if (a[0] !== 'api' && a[0] !== 'auth') { process.stderr.write('gh-bobix: only api and auth\n'); process.exit(2); }
  if (a[0] === 'api' && a.some(x => /^-X$|^--method$/.test(x))) { process.stderr.write('gh-bobix: read only\n'); process.exit(2); }
  process.stdout.write(run(a));
}
