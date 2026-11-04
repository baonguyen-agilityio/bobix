# Roadmap and open work

What Bobix still lacks, what is done but not released, and what nobody has verified yet. Update this file when an item lands, ships or changes. Last updated: 2026-11-04 (v0.1.2).

## 1. Done on `main`, not released yet

- **Version shown in the app**: next to the name in the panel, and at the top of the tray menu.
- **System messages aren't quests**: what Claude Code injects as a prompt (a background task ended, a reminder) shows as a short "System:" log line and no longer replaces the quest or brings back a hidden pet.
- **Session box**: spacing under the buttons, log times on one line (24-hour).

Everything before that is in v0.1.2 (2026-11-04), see its [release notes](https://github.com/baonguyen-agilityio/bobix/releases/tag/v0.1.2).

Release checklist: bump `version` in `package.json` → tag `vX.Y.Z` → check the 5 fixed-name files are attached → add release notes.

## 2. Not verified on a real machine

| What | Where it is tested so far | Missing |
|---|---|---|
| Windows: pets for real Claude Code sessions, Open session | CI smoke only | A run on real Windows with a Claude Code session (Code tab or CLI, not chat), plus **Copy diagnostics** output. The earlier "could not find that session's window" on Windows has no known cause yet. |
| Approve from Bobix | macOS, app run from source | Windows and Ubuntu; the buttons on macOS notifications (unsigned builds may not show them) |
| Drag a pet | macOS, CI (real mouse via xdotool) | A real Ubuntu desktop |
| Side sessions on Windows and Linux | macOS only | Where the Claude desktop app keeps `claude-code-sessions` there (assumed `%APPDATA%\\Claude` and `~/.config/Claude`) |
| Claude desktop app on Linux (beta, Ubuntu / Debian) | Not tested | Whether a click on a pet opens its session there (the `claude://` link and the session id the hook reads), and where it keeps session metadata (needed for parent and side sessions) |
| New version notice | Unit tests and the real GitHub API from a script | Seeing the notification and tray item in a packaged app (first possible when a release newer than v0.1.2 is out) |

## 3. Planned features

Small, they reuse data Bobix already has. Good to do together:

- [ ] **Remind when stuck**: a session waiting for approval longer than N minutes notifies again, and its pet changes look.
- [ ] **Global shortcut**: jump to the next session that is waiting for you.
- [ ] **Cost per session**: tokens used (from the transcript), and a total for the day.
- [ ] **Stop / resume**: stop a running session (interrupt its claude process); copy `claude --resume <id>` for a session whose terminal is closed.

Larger, each needs its own plan:

- [ ] **Several machines (peer)**: see pets of sessions on a devbox or a second computer, labeled with the machine name. First version: the remote hook sends to your laptop over a private network (Tailscale / LAN) or an SSH tunnel, paired with a one-time code, view only. Approving from another machine is a separate opt-in, because it changes the security model (today Bobix only accepts connections from the same computer).
- [ ] **Daily recap**: what each session did today and its last answer.
- [ ] **Group by project**: pets of the same folder stand together; the panel groups sessions by repository.
- [ ] **Share with a team (view only)**: after "several machines"; prompts and paths hidden by default.

## 4. Known limits (by design or outside Bobix's control)

- Builds are not code-signed: macOS and Windows warn on first launch.
- Linux Wayland: no app may bring another app's window forward, so a click on a terminal session's pet opens it in the panel and names the terminal instead. Sessions in the Claude desktop app open directly.
- Opening a session in the Claude desktop app uses `claude://code/continue`, not a documented API; it may change.
- While Bobix waits for an answer (20 s by default), the terminal's permission prompt appears only after that time.
- Approvals need the Node.js forwarder; with the curl / PowerShell fallback Bobix only watches.
- Linux installers are `.deb` only (Ubuntu, Debian). Other distributions aren't packaged.
- Only Claude Code sessions (terminal, IDE, the Code tab of the Claude app). Ordinary chats in the Claude app or on claude.ai have no hooks, so Bobix can't follow them.
  - Checked on 2026-11-04 on macOS while a chat answered: Claude desktop's logs (`~/Library/Logs/Claude`) record nothing about chats, and the only chat-related writes are to claude.ai's internal IndexedDB, which keep happening after the answer ends, so they can't serve as a status. Reading that database would mean reading private chat data in an undocumented format.
  - Not tried, decided against for now: reading the app's UI through the Accessibility API (e.g. whether a "Stop" button is showing; needs the Accessibility permission and breaks when the UI changes), and a browser extension for claude.ai on the web (does not cover the desktop app).
- Memory is about 330–400 MB (Electron).

## 5. Cleanup

- [x] Re-recorded the demo video and screenshots (2026-11-04): the waiting pet now shows Allow / Deny. Drag and hide/show are not in the video.
- [x] Removed the old versioned file names from the v0.1.1 release; only the fixed names remain.
- [x] Deleted the merged branch `fix/pets-overlay-win-linux`.
- [ ] The from-source Ubuntu CI smoke failed once with the pets page not yet drawn. It has passed since; watch for it.
- [x] On the maintainer's Mac: v0.1.2 installed in Applications, login item moved to it, old build folder in the Trash (2026-11-04).
