// state.mjs: one snapshot of the scratch app's sessions (title, state, context left, issue, tool), for planning shots.
import fs from 'node:fs';
const token = fs.readFileSync(process.env.W + '/home/.claude/bobix/token', 'utf8').trim();
const r = await fetch(`http://127.0.0.1:4791/events?t=${token}`), rd = r.body.getReader();
let buf = '';
while (!buf.includes('\n\n')) buf += new TextDecoder().decode((await rd.read()).value);
const d = JSON.parse(buf.split('\n\n')[0].replace(/^data: ?/gm, ''));
for (const s of d.sessions) console.log([s.name || s.title, s.state, 'ctx=' + s.ctx, s.issue ? '#' + s.issue.iid : '', s.tool || '', s.parent ? 'parent=' + s.parent : ''].join(' | '));
if (process.argv[2]) console.log(JSON.stringify(Object.keys(d)), JSON.stringify(Object.keys(d.sessions[0] || {})));
process.exit(0);
