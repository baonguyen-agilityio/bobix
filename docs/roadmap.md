# Roadmap and open work

What Bobix still lacks, what is done but not released, and what nobody has verified yet. Update this file when an item lands, ships or changes. Last updated: 2026-11-04 (after v0.1.1).

## 1. Done on `main`, not released yet (next: v0.1.2)

- **New version notice**: a daily check of GitHub's latest release, a notification once per version, a tray item to download it, and a Settings switch. Users on v0.1.1 or older won't see it until they install a version that has it.
- **Permission hook fixes**:
  - the hook never waits longer than Bobix's own wait time, and gives up after 3 s if Bobix is frozen (it could wait up to 5 minutes before);
  - `claude -p` and Agent SDK runs (`CLAUDE_CODE_ENTRYPOINT=sdk-*`) never wait;
  - the hook exits the moment an answer arrives (before, an approved request still waited out the full time).
- **Setup wording**: says that ordinary Claude chats don't show up, and that nothing about sessions leaves the computer.
- **Docs**: install only from release installers (fixed file names, `releases/latest/download/…` links, terminal and AI-assistant commands), building from source moved to [development.md](development.md), privacy doc updated for the update check and the waiting permission hook.

Release checklist: bump `version` in `package.json` → tag `v0.1.2` → check the 5 fixed-name files are attached → add release notes.

## 2. Not verified on a real machine

| What | Where it is tested so far | Missing |
|---|---|---|
| Windows: pets for real Claude Code sessions, Open session | CI smoke only | A run on real Windows with a Claude Code session (Code tab or CLI, not chat), plus **Copy diagnostics** output. The earlier "could not find that session's window" on Windows has no known cause yet. |
| Approve from Bobix | macOS, app run from source | Windows and Ubuntu; the buttons on macOS notifications (unsigned builds may not show them) |
| Drag a pet | macOS, CI (real mouse via xdotool) | A real Ubuntu desktop |
| New version notice | Unit tests and the real GitHub API from a script | Seeing the notification and tray item in a packaged app (needs a release newer than the installed one) |

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
- [ ] On the maintainer's Mac: an old packaged build in the project's build folder is still set to start at login. After v0.1.2 is out, install it into Applications, then remove the old entry in System Settings → General → Login Items (turn "Start at login" on again in the new Bobix if wanted).
