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

Bobix runs on your computer. Its local server listens on `127.0.0.1` only, and nothing about you or your sessions leaves
the machine (what it reads, writes and sends is in [docs/privacy.md](docs/privacy.md)). A report about that boundary, the
local server, the hooks Bobix installs in `~/.claude/settings.json`, or a file Bobix writes is in scope.
