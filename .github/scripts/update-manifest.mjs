// Writes the update manifest the app reads (docs/development.md, "Update feed"): one JSON file naming, per system, the
// update file and its signature. The release workflow runs it on a tag; the local update test runs it with --base on a
// folder served from this computer. The names are the ones the release workflow gives the update files.
//   node update-manifest.mjs --dir out --tag v0.2.0-rc.8 --base https://github.com/OWNER/REPO/releases/download/v0.2.0-rc.8 --out latest.json [--partial]
//        [--conf src-tauri/tauri.conf.json --package package.json]   (the release job: also checks the key and the version, below)
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

// minisign's key id as minisign prints it ("minisign public key: 6C8C…"): the files hold base64 of text whose second line is
// base64 of 2 bytes of algorithm, 8 of key id (stored the other way round), then the key or signature.
export function keyId(b64) {
  const line = Buffer.from(b64.trim(), 'base64').toString('utf8').split('\n')[1] || '';
  const bytes = Buffer.from(line.trim(), 'base64');
  return bytes.length >= 10 ? Buffer.from(bytes.subarray(2, 10)).reverse().toString('hex').toUpperCase() : '';
}

// What a release must be before it is published, or no copy can ever update from it: a real public key in the app (not the
// placeholder), the tag the version the app says it is, and every update file signed with the key that public key belongs to.
export function checkRelease({ conf, pkg, tag, signatures }) {
  const pubkey = conf?.plugins?.updater?.pubkey || '';
  const id = keyId(pubkey);
  if (!Buffer.from(pubkey, 'base64').toString('utf8').startsWith('untrusted comment:') || !id) {
    throw new Error('plugins.updater.pubkey in tauri.conf.json is not a public key (still the placeholder?): copies of this release could never update');
  }
  if (tag !== `v${pkg.version}`) throw new Error(`the tag ${tag} is not package.json's version v${pkg.version}: the app would report one version and the update list another`);
  for (const [name, sig] of Object.entries(signatures)) {
    if (keyId(sig) !== id) throw new Error(`${name}.sig was signed with key ${keyId(sig) || '?'}, not the key ${id} of the public key in tauri.conf.json`);
  }
}

// partial: leave out the systems that have no file here (a local test builds one). Otherwise a missing file or
// signature is an error: a release without them would break updating for that system.
export function manifest({ dir, tag, base, partial = false, date = new Date(), conf, pkg }) {
  const platforms = {}, signatures = {};
  for (const [key, name] of Object.entries(FILES)) {
    const file = join(dir, name), sig = file + '.sig';
    if (!existsSync(file) || !existsSync(sig)) {
      if (partial) continue;
      throw new Error(`missing ${name} or ${name}.sig in ${dir}: the update files are signed only when the signing key is set`);
    }
    signatures[name] = readFileSync(sig, 'utf8').trim();
    platforms[key] = { signature: signatures[name], url: `${base.replace(/\/$/, '')}/${name}` };
  }
  if (!Object.keys(platforms).length) throw new Error(`no update file in ${dir}`);
  if (conf && pkg) checkRelease({ conf, pkg, tag, signatures });
  return { version: tag.replace(/^v/, ''), notes: `https://github.com/${process.env.GITHUB_REPOSITORY || 'baonguyen-agilityio/bobix'}/releases/tag/${tag}`, pub_date: date.toISOString(), platforms };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const a = Object.fromEntries(process.argv.slice(2).filter(x => x.startsWith('--')).map(x => [x.slice(2), true]));
  const opt = k => { const i = process.argv.indexOf('--' + k); return i > 0 ? process.argv[i + 1] : undefined; };
  const json = f => (opt(f) ? JSON.parse(readFileSync(opt(f), 'utf8')) : undefined);
  const m = manifest({ dir: opt('dir'), tag: opt('tag'), base: opt('base'), partial: !!a.partial, conf: json('conf'), pkg: json('package') });
  writeFileSync(opt('out'), JSON.stringify(m, null, 2) + '\n');
  console.log(`${opt('out')}: ${m.version}, ${Object.keys(m.platforms).join(', ')}`);
}
