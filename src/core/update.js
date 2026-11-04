// "A new version is available": reads the latest release from GitHub's public API (no account, nothing sent
// but the request itself) and compares its version with the running one. Pre-releases are never offered.
const parse = v => (String(v).match(/^v?(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$/) || []).slice(1).map(Number);

// True when `latest` (a tag like "v0.1.2") is a newer release than `current` ("0.1.1" or "0.1.1-rc.3").
function isNewer(latest, current) {
  const a = parse(latest), b = parse(current);
  if (a.length !== 3 || b.length !== 3) return false;
  for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] > b[i];
  return /-/.test(String(current)) && !/-/.test(String(latest)); // 0.1.1 is newer than 0.1.1-rc.3
}

// Resolves { version, url } when a newer release exists, else null. Never throws: offline or GitHub down → null.
async function checkLatest({ repo, current, fetchImpl = fetch, timeoutMs = 10000 }) {
  try {
    const res = await fetchImpl(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': `Bobix/${current}` },
      signal: AbortSignal.timeout(timeoutMs),
    });
    if (!res.ok) return null;
    const r = await res.json();
    if (r.draft || r.prerelease || !isNewer(r.tag_name, current)) return null;
    const page = `https://github.com/${repo}/releases/`;
    return { version: String(r.tag_name).replace(/^v/, ''), url: typeof r.html_url === 'string' && r.html_url.startsWith(page) ? r.html_url : page + 'latest' };
  } catch {
    return null;
  }
}

module.exports = { isNewer, checkLatest };
