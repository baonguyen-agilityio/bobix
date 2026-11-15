// Says whether a build has phone access (the `phones` Cargo feature), from the git ref release.yml runs on.
//   node build-kind.mjs refs/tags/v0.2.0-rc.8   ->  "rc phones=on"
// A branch (develop, or a run by hand) and a "-rc.N" tag build it in; a stable "vX.Y.Z" tag builds it out. A tag that is
// neither (a malformed one, another pre-release word) takes it OUT: only a tag can become a release, so the switch never
// comes on there by accident.
export function kindOf(ref) {
  const tag = /^refs\/tags\/(.+)$/.exec(ref || '')?.[1];
  if (tag === undefined) return { kind: ref === 'refs/heads/develop' ? 'develop' : 'branch', phones: true }; // a branch never becomes a stable release
  if (/^v\d+\.\d+\.\d+$/.test(tag)) return { kind: 'stable', phones: false };
  if (/^v\d+\.\d+\.\d+-rc\.\d+$/.test(tag)) return { kind: 'rc', phones: true };
  return { kind: 'other', phones: false };
}

import { fileURLToPath } from 'node:url';
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const k = kindOf(process.argv[2]);
  console.log(`${k.kind} phones=${k.phones ? 'on' : 'off'}`);
}
