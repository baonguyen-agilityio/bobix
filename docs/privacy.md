# Privacy and security

Bobix runs entirely on your computer. It has no account and no analytics. Its only network request on its own asks GitHub, once a day, whether a newer version exists; you can turn that off. The panel can also read your GitLab issues, only if you turn it on (off by default).

## What it reads

| Source | What | Why |
|--------|------|-----|
| Claude Code hook events | session id, project folder, your prompt, tool name and its path / command / pattern, permission requests, the claude process id, and in the Claude desktop app its session id | to show what each session is doing and open it on click |
| The Claude desktop app's session file for a session it runs (`…/Claude/claude-code-sessions/…/<id>.json`) | only `spawnedFrom.sessionId`, the session it was started from; while GitLab is on, also each session's title, folder, last activity and whether it is archived | to show side sessions next to their parent, and to find the session you started from an issue (by its name) for Resume |
| The session's git repository, with `git` (local only, no fetch) | branch, upstream and ahead / behind, the number of changed files, commits on no remote, the last commit's subject and time, the number of worktrees, the remote URLs | the Git part of a session's details, and (GitLab on) which GitLab project and issue it works on. Remote URLs stay in the app's main process and are never shown or sent: they can hold credentials. |
| The session transcript (`~/.claude/projects/…jsonl`) | the last ~512 KB (and the first 256 KB, for the title): the final answer, token usage, the session title | the "report" line, the context meter, the pet's name |

The hook script drops file contents and tool output before sending anything. All of this is kept **in memory only** and forgotten when a session ends or the app quits.

## What it writes

| File | Content |
|------|---------|
| `~/.claude/settings.json` | Bobix's hook entries (only when you click Install hooks; removed by Remove hooks) |
| `~/.claude/settings.json.bak-bobix-*` | a copy of that file before each change, same permissions, newest 3 kept. It contains whatever your settings file contains. |
| `~/.claude/bobix/hook.js`, `token`, `header` | the forwarder and a random per-user token (mode 0600) |
| The app's settings folder → `settings.json` | your Bobix preferences, including the short code you set for a project, which picture each project uses and the pixel-art settings you chose for it |
| The app's settings folder → `pets/` | the pixel-art version of each project's picture (the face the pets wear), and in `pets/originals/` the original picture as you chose it, kept so Edit picture can start from it and never shown or served by Bobix; both removed when no project uses them |
| Start at login (only if you turn it on) | a login item (macOS / Windows), a LaunchAgent (macOS, from source) or `~/.config/autostart/bobix.desktop` (Linux) |

## What it sends, and to whom

- **Nothing about you or your sessions leaves your computer**, unless you send feedback yourself (below). Fonts and images are bundled.
- **New version check** (Settings → "Tell me about new versions", on by default): once a day the app reads `https://api.github.com/repos/baonguyen-agilityio/bobix/releases/latest`, GitHub's public page of the latest release. It sends no data, only the request itself, so GitHub sees what any web page sees: your IP address and `Bobix/<version>` as the program name. No account or token is used.
- **Feedback** (the Feedback button at the top of the panel, only when you use it): opens GitHub's new-issue form in your browser with your text filled in. Nothing is sent until you submit it there, with your GitHub account, and the issue is public. A bug report also fills in technical details: the app version, operating system, hook state, some settings, session states and recent errors with folders and tokens removed. You can read them before opening the form. No prompts, code, project names or tokens. An idea carries no technical details.
- **GitLab issues** (Settings → Work (GitLab, GitHub): "Show my GitLab issues", **off by default**): while the switch is on, Bobix runs your own `glab auth status` to learn which GitLab hosts you are signed in to (it reads only the "Logged in to <host> as" lines; glab masks the token there), then for each of them `glab api --hostname <host> issues?scope=assigned_to_me&state=opened&per_page=100` when you turn it on, when the app starts, every 5 minutes and when you press Refresh. That is one read (GET) per host of the open issues assigned to you. With it, two more reads per host: `merge_requests?scope=created_by_me&state=opened&per_page=100` (your open merge requests: an issue one of them closes, by "Closes #n" in its description or a branch named `<n>-…`, waits for review) and `issues?scope=assigned_to_me&state=closed&updated_after=<4 days ago>&per_page=100` (the board keeps those closed today, for "Done today" and the report, and those closed on the last working day, for the stand-up). Bobix never writes to GitLab. `glab` uses the sign-in you made yourself (`glab auth login --hostname <host>`); Bobix never signs in for you, never reads or stores the token, and sends nothing about your prompts, sessions or code, only that request. It keeps the issues' number, title, project, labels, due date and link in memory. Off, Bobix makes no GitLab request at all. Links open in your browser, and only links of the issues it read.
- **GitHub issues** (Settings → Work (GitLab, GitHub): "Show my GitHub issues", **off by default**): the same, through your own `gh`. Bobix runs `gh auth status` to learn which GitHub hosts (github.com, GitHub Enterprise) you are signed in to; it reads only the "Logged in to <host> account <name>" lines and the "Active account" line after each (gh masks the token there), so with several accounts on one host it uses the active one, as `gh` itself does. Then for each host `gh api --hostname <host> issues?filter=assigned&state=open&per_page=100`, at the same times as GitLab: one read (GET) per host of the open issues assigned to you; pull requests in that answer are dropped. With it, `gh api --hostname <host> graphql` with a query (no mutation) for your open pull requests and the issues each one closes, and `issues?filter=assigned&state=closed&since=<4 days ago>&per_page=100` for the issues closed today and on the last working day. It keeps a pull request's link and the issue numbers it closes. Bobix never writes to GitHub, never signs in for you (`gh auth login`), never reads or stores the token, and keeps the same fields in memory (no due date: GitHub issues have none). Off, Bobix makes no request through gh at all.
- Hooks talk to the app over `127.0.0.1` only. The server checks the Host header (blocks DNS rebinding) and requires the per-user token on every event and on the state stream, so web pages and other users on the same machine can neither feed it nor read it.
- **Notifications** show a session's request or the start of its final answer through your operating system. They can appear while you share your screen; turn them off or use **Pause notifications** in Settings.

## What it runs

- `node ~/.claude/bobix/hook.js <port>` on each Claude Code hook event, in the background so Claude never waits for it. SessionStart runs in the foreground (5 s at most, a local call) so it can name a session started from an issue; Claude lets you type meanwhile. The one exception is a permission request: there the hook waits for your answer in Bobix (Settings → "Approve from Bobix": by default until you answer there, or a time limit you set; off if you choose). Claude Code shows its own prompt in the terminal at the same time, so you can answer there instead. Runs without a person at Bobix (`claude -p`, Agent SDK apps) never wait. Answers come only from Bobix's own windows and notifications, never over the network. Fallbacks without Node.js: `curl` or PowerShell (no approvals).
- When you click a pet: `open -a <app>` (macOS), `WScript.Shell.AppActivate` via PowerShell (Windows) or `xdotool` (Linux) to bring the session's window forward, and `ps` / `Get-CimInstance` to find that window's process. In the Claude desktop app it opens `claude://code/continue?session=<id>`, a link that app registers. It is not a documented public API and may change; if it stops working, the click falls back to the session's details.
- For each folder a session works in, at most every 30 s (and on a session's start and end): `git status --porcelain=v2 --branch`, `git rev-parse --show-toplevel`, `git log -1`, `git rev-list --count HEAD --not --remotes`, `git worktree list --porcelain`, `git remote -v`. Read-only, without taking git's index lock; nothing is fetched or pushed.
- Only while GitLab is on: `glab auth status` and `glab api …` as above, and once a login shell (`command -v glab`) to find where glab is installed. The same for GitHub with `gh` while GitHub is on.
- **New** on an issue in the panel's To do column (only when you press it): opens `claude://code/new?folder=<the project's folder>`, Claude's documented link for a new session (the Claude app asks you to confirm the folder). No prompt or other text is sent. The first session then created in that folder (SessionStart with source `startup`) is linked to the issue, and Bobix's SessionStart hook answers with its name (`[<code>-<number>] <short title>`, as `/rename` would set it); this is the only hook output besides permission answers. Until then the issue shows "waiting for a new session" with ✕ to cancel. **Resume** does what a click on the session's pet does, or, for a session Bobix doesn't see running, opens `claude://code/continue?session=<id>` for the newest Claude desktop session in that folder with the issue's name.
- Arguments are always passed as lists, never through a shell.

## Your organization's policies

Bobix only uses Claude Code's documented hooks. If your organization's managed settings allow only managed hooks (`allowManagedHooksOnly`), Claude Code does not run Bobix's hooks and the app receives nothing. Bobix does not try to work around that.

## Affiliation

Bobix is an independent open-source project. It is not affiliated with, endorsed by, or sponsored by Anthropic. "Claude" and "Claude Code" are trademarks of Anthropic, used here only to say what the app works with.

## Reporting a problem

If you find a security issue, please report it privately to the maintainer instead of opening a public issue.
