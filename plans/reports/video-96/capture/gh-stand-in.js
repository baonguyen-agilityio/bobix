#!/usr/bin/env node
// Stand-in gh for the evidence run: realistic, made-up issues and pull requests of a shop team (read-only answers).
const a = process.argv.slice(2), now = Date.now(), H = 3600e3, D = 24 * H, iso = t => new Date(t).toISOString();
const out = x => process.stdout.write(typeof x === 'string' ? x : JSON.stringify(x));
if (a[0] === 'auth') { out('github.com\n  ✓ Logged in to github.com account dev-bobo (keyring)\n  - Active account: true\n'); process.exit(0); }
const I = (p, n, title, labels = [], o = {}) => ({ number: n, title, html_url: `https://github.com/${p}/issues/${n}`, repository_url: `https://api.github.com/repos/${p}`,
  labels: labels.map(name => ({ name })), state: 'open', body: `Steps and acceptance criteria for "${title}".`, ...o });
const W = 'acme/shop-web', A = 'acme/shop-api', DS = 'acme/design-system';
const open = [
  I(W, 41, 'Checkout: apply coupon codes at the payment step', ['enhancement']),
  I(W, 44, 'Cart badge shows a stale count after removing an item', ['bug']),
  I(W, 47, 'Translate the order history page into Vietnamese', ['enhancement']),
  I(W, 49, 'Product images load full size on mobile', ['bug', 'performance']),
  I(W, 52, 'Order confirmation email lacks the delivery date', ['bug']),
  I(A, 12, 'Rate-limit the password reset endpoint', ['security']),
  I(A, 15, 'Return 404 instead of 500 for an unknown product id', ['bug']),
  I(A, 18, 'Test cases for refunds after 30 days', ['qa']),
  I(DS, 7, 'Button: add a loading state', ['enhancement']),
  I(DS, 9, 'Document the spacing tokens', ['docs']),
];
const closed = [I(A, 10, 'Paginate the orders list', [], { state: 'closed', closed_at: iso(now - 2 * H) }), I(W, 39, 'Header search clears on Back', ['bug'], { state: 'closed', closed_at: iso(now - 4 * H) })];
const unassigned = { [`${W}#58`]: I(W, 58, 'Saved addresses: let a customer pick a default', ['enhancement']) };
const pr = (repo, n, branch, closes, created, updated, reviews = 0) => ({ number: n, url: `https://github.com/${repo}/pull/${n}`, headRefName: branch, body: `Closes #${closes}`,
  createdAt: iso(created), updatedAt: iso(updated), reviewDecision: null, reviews: { totalCount: reviews }, repository: { nameWithOwner: repo },
  closingIssuesReferences: { nodes: [{ number: closes, repository: { nameWithOwner: repo } }] } });
const path = a[a.length - 1] || '';
if (process.env.EV_QUIET) { closed.length = 0; } // a quiet morning: nothing closed, no pull requests
if (a.includes('graphql')) out({ data: { viewer: { pullRequests: { nodes: process.env.EV_QUIET ? [] : [pr(W, 60, '52-delivery-date', 52, now - 3 * H, now - 2 * H), pr(DS, 31, '7-button-loading', 7, now - 30 * D, now - 20 * D)] } } } });
else if (path.startsWith('issues?filter=assigned&state=open')) out(open);
else if (path.startsWith('issues?filter=assigned&state=closed')) out(closed);
else { const m = path.match(/^repos\/(.+)\/issues\/(\d+)$/); const k = m && `${m[1]}#${m[2]}`; const x = k && (unassigned[k] || open.find(i => `${i.repository_url.split('/repos/')[1]}#${i.number}` === k));
  if (x) out(x); else { process.stderr.write('HTTP 404: Not Found'); process.exit(1); } }
