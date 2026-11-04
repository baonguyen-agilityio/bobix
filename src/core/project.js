// Which project a session works in: the git repository its folder belongs to, so that every session of a
// project (and every git worktree of it, which Claude desktop makes per session) gets the same pet.
const fs = require('fs');
const path = require('path');

const cache = new Map();

// The main checkout of the repository that contains `cwd`, or `cwd` itself outside git.
function projectOf(cwd) {
  if (typeof cwd !== 'string' || !path.isAbsolute(cwd)) return cwd || '';
  if (cache.has(cwd)) return cache.get(cwd);
  let key = cwd;
  for (let dir = cwd, i = 0; i < 40; i++, dir = path.dirname(dir)) {
    const dotGit = path.join(dir, '.git');
    let st;
    try { st = fs.statSync(dotGit); } catch { if (path.dirname(dir) === dir) break; continue; }
    if (st.isDirectory()) { key = dir; break; }
    // A worktree: ".git" is a file "gitdir: <main>/.git/worktrees/<name>", whose "commondir" leads to <main>/.git.
    try {
      const gitdir = path.resolve(dir, /^gitdir:\s*(.+)$/m.exec(fs.readFileSync(dotGit, 'utf8'))[1].trim());
      let common = gitdir;
      try { common = path.resolve(gitdir, fs.readFileSync(path.join(gitdir, 'commondir'), 'utf8').trim()); } catch {}
      key = path.basename(common) === '.git' ? path.dirname(common) : dir;
    } catch { key = dir; }
    break;
  }
  if (cache.size > 500) cache.clear();
  cache.set(cwd, key);
  return key;
}

module.exports = { projectOf };
