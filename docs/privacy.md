# Privacy and security

Bobix runs entirely on your computer. It has no account and no analytics. Its only network request asks GitHub, once a day, whether a newer version exists; you can turn that off.

## What it reads

| Source | What | Why |
|--------|------|-----|
| Claude Code hook events | session id, project folder, your prompt, tool name and its path / command / pattern, permission requests, the claude process id, and in the Claude desktop app its session id | to show what each session is doing and open it on click |
| The Claude desktop app's session file for a session it runs (`…/Claude/claude-code-sessions/…/<id>.json`) | only `spawnedFrom.sessionId`, the session it was started from | to show side sessions next to their parent |
| The session transcript (`~/.claude/projects/…jsonl`) | the last ~512 KB (and the first 256 KB, for the title): the final answer, token usage, the session title | the "report" line, the context meter, the pet's name |

The hook script drops file contents and tool output before sending anything. All of this is kept **in memory only** and forgotten when a session ends or the app quits.

## What it writes

| File | Content |
|------|---------|
| `~/.claude/settings.json` | Bobix's hook entries (only when you click Install hooks; removed by Remove hooks) |
| `~/.claude/settings.json.bak-bobix-*` | a copy of that file before each change, same permissions, newest 3 kept. It contains whatever your settings file contains. |
| `~/.claude/bobix/hook.js`, `token`, `header` | the forwarder and a random per-user token (mode 0600) |
| The app's settings folder → `settings.json` | your Bobix preferences, including which picture each project uses |
| The app's settings folder → `pets/` | pictures you chose for projects, scaled to 128 px (GIFs copied as they are); removed when no project uses them |
| Start at login (only if you turn it on) | a login item (macOS / Windows), a LaunchAgent (macOS, from source) or `~/.config/autostart/bobix.desktop` (Linux) |

## What it sends, and to whom

- **Nothing about you or your sessions leaves your computer.** Fonts and images are bundled.
- **New version check** (Settings → "Tell me about new versions", on by default): once a day the app reads `https://api.github.com/repos/baonguyen-agilityio/bobix/releases/latest`, GitHub's public page of the latest release. It sends no data, only the request itself, so GitHub sees what any web page sees: your IP address and `Bobix/<version>` as the program name. No account or token is used.
- Hooks talk to the app over `127.0.0.1` only. The server checks the Host header (blocks DNS rebinding) and requires the per-user token on every event and on the state stream, so web pages and other users on the same machine can neither feed it nor read it.
- **Notifications** show a session's request or the start of its final answer through your operating system. They can appear while you share your screen; turn them off or use **Pause notifications** in Settings.

## What it runs

- `node ~/.claude/bobix/hook.js <port>` on each Claude Code hook event, in the background so Claude never waits for it. The one exception is a permission request: there the hook waits for your answer in Bobix (Settings → "Approve from Bobix", 20 s by default, 0 = off), then Claude asks in the terminal as usual. Runs without a person at Bobix (`claude -p`, Agent SDK apps) never wait. Answers come only from Bobix's own windows and notifications, never over the network. Fallbacks without Node.js: `curl` or PowerShell (no approvals).
- When you click a pet: `open -a <app>` (macOS), `WScript.Shell.AppActivate` via PowerShell (Windows) or `xdotool` (Linux) to bring the session's window forward, and `ps` / `Get-CimInstance` to find that window's process. In the Claude desktop app it opens `claude://code/continue?session=<id>`, a link that app registers. It is not a documented public API and may change; if it stops working, the click falls back to the session's details.
- Arguments are always passed as lists, never through a shell.

## Your organization's policies

Bobix only uses Claude Code's documented hooks. If your organization's managed settings allow only managed hooks (`allowManagedHooksOnly`), Claude Code does not run Bobix's hooks and the app receives nothing. Bobix does not try to work around that.

## Affiliation

Bobix is an independent open-source project. It is not affiliated with, endorsed by, or sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic, used here only to say what the app works with.

## Reporting a problem

If you find a security issue, please report it privately to the maintainer instead of opening a public issue.
