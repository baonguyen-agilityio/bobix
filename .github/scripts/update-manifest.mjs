// Writes the update manifest the app reads (docs/development.md, "Update feed"): one JSON file naming, per system, the
// update file and its signature. The release workflow runs it on a tag; the local update test runs it with --base on a
// folder served from this computer. The names are the ones the release workflow gives the update files.
//   node update-manifest.mjs --dir out --tag v0.2.0-rc.8 --base https://github.com/OWNER/REPO/releases/download/v0.2.0-rc.8 --out latest.json [--partial]
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// manifest key → [update file, its signature]. The updater adds the installer's name for Linux (`-deb`): a .deb install
// asks for the password, an AppImage would not.
export const FILES = {
  'darwin-aarch64': 'Bobix-mac-arm64.app.tar.gz',
  'darwin-x86_64': 'Bobix-mac-x64.app.tar.gz',
  'windows-x86_64': 'Bobix-windows-x64.exe',
  'linux-x86_64-deb': 'bobix-linux-amd64.deb',
  'linux-aarch64-deb': 'bobix-linux-arm64.deb',
};

// partial: leave out the systems that have no file here (a local test builds one). Otherwise a missing file or
// signature is an error: a release without them would break updating for that system.
export function manifest({ dir, tag, base, partial = false, date = new Date() }) {
  const platforms = {};
  for (const [key, name] of Object.entries(FILES)) {
    const file = join(dir, name), sig = file + '.sig';
    if (!existsSync(file) || !existsSync(sig)) {
      if (partial) continue;
      throw new Error(`missing ${name} or ${name}.sig in ${dir}: the update files are signed only when the signing key is set`);
    }
    platforms[key] = { signature: readFileSync(sig, 'utf8').trim(), url: `${base.replace(/\/$/, '')}/${name}` };
  }
  if (!Object.keys(platforms).length) throw new Error(`no update file in ${dir}`);
  return { version: tag.replace(/^v/, ''), notes: `https://github.com/${process.env.GITHUB_REPOSITORY || 'baonguyen-agilityio/bobix'}/releases/tag/${tag}`, pub_date: date.toISOString(), platforms };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const a = Object.fromEntries(process.argv.slice(2).filter(x => x.startsWith('--')).map(x => [x.slice(2), true]));
  const opt = k => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : undefined; };
  const m = manifest({ dir: opt('dir'), tag: opt('tag'), base: opt('base'), partial: !!a.partial });
  writeFileSync(opt('out'), JSON.stringify(m, null, 2) + '\n');
  console.log(`${opt('out')}: ${m.version}, ${Object.keys(m.platforms).join(', ')}`);
}
