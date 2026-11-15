# Security policy

## Reporting a vulnerability

Please report it privately, not in a public issue or pull request:

1. Open the repository's **Security** tab.
2. Choose **Report a vulnerability** (GitHub's private vulnerability reporting).
3. Say what you found, how to reproduce it, and which version of Bobix it affects.

Only the maintainers can see the report. You will get an answer there; please keep the details private until a fix is released.

## Supported versions

The latest release, and `develop` for the next one. The v1 line (0.1.x) ends with 0.2.0 and gets no more fixes.

## What Bobix stores

Bobix runs on your computer. Its local server listens on `127.0.0.1` only (a war room you host adds a second port, on your
Tailscale address only), and nothing goes to Bobix's author. Text goes to Anthropic only for the features you switch on or
press, through your own Claude. What it reads, writes and sends is in [docs/privacy.md](docs/privacy.md). A report about that boundary, the
local server, the hooks Bobix installs in `~/.claude/settings.json`, or a file Bobix writes is in scope.

Phone access (in development; only builds from the `feat/phone-access` branch have it) is in scope as well: the relay in `relay/` (what it accepts before a device authenticates,
its limits, push grants and the gateway), the sealed packets and signed commands of `crates/bobix-link/`, pairing and the rights
a phone is given, and the keys Bobix keeps for it. The phone app in `mobile/` is in scope too: how it stores its keys and uses
the biometric gate, its app lock, how it draws a plan or a tool input, and what its notification extension and push service do
with a sealed notice. The app has not been security-reviewed on a real iPhone or Android phone yet. The relay's own deployment (your host, your domain) is yours to secure; a
weakness in the code that every deployment runs is what to report.
