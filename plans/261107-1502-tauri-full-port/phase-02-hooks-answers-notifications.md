# Phase 2: hooks, Allow/Deny, notifications, open session

Back to [plan](plan.md). Still Electron for users: everything.

## Context
- `src/core/hooks.js`, `src/hook/hook.js` (unchanged), `src/core/focus.js`, `main.js` (`waitForAnswer`, `answer`, `releaseUnseen`, `notify`, `openSession`, `findBin`, `installHooks`, `titleFor`, hook refresh at boot).

## Requirements
- `hooks.rs`: `install`/`uninstall`/`status`/`strip`/`isOurs`/`hookEntry` byte-for-byte compatible JSON (node, curl, PowerShell forms; PermissionRequest foreground with 86400 cap; SessionStart foreground), backups (keep 3), write through symlinks keeping the mode, refuse invalid JSON, remove the old `session-guild` folder. `BOBIX_HOME` override for tests and dev.
- `hook.js` embedded and copied to `~/.claude/bobix/hook.js`; at start: reinstall when outdated or stale, else refresh the copy (as today).
- `findBin(node|glab)`: `where` on Windows, login shell then interactive login shell elsewhere.
- Permission requests: held `/event` reply with `x-bobix-wait` (`secs` or `forever`), done on answer / timeout / client gone; one open request per session; `releaseUnseen` on settings and pet changes; answers only from our pages (and the notification).
- SessionStart answers with a session title for an issue branch (#41) — needs the GitLab issue list from phase 3; until then it answers 204 like "no title".
- Notifications: waiting / done, respecting `paused`, `notifyWaiting`, `notifyDone`, `sound`; click opens the session; macOS Allow/Deny buttons for a held request.
- `focus.rs`: `ancestors`, `focusPids`, `processNames`, `hostApp`, `outerBundle` with today's commands and timeouts; `openSession` with `claude://code/continue?session=…`, `claude://` for the Claude app's terminal, reasons `nopid | wayland | xdotool | notfound`.

## Files
- Create: `src-tauri/src/{hooks.rs,answers.rs,notify.rs,focus.rs,bins.rs}`.
- Edit: `server.rs` (held replies), `main.rs`, `bridge.js` (`answer`, `petClick`).

## Steps
1. Port hooks + tests ("hooks install keeps foreign hooks…", symlink/mode, outdated rules, Session Guild upgrade).
2. Held replies + answers; keep the node tests that run the real `hook.js` against the Rust server (spawned test binary) for "waits for the answer", "headless never held".
3. Notifications per OS (try the notification plugin first; if it lacks desktop click/actions, use `mac-notification-sys` / `notify-rust` / WinRT directly).
4. Focus + open session; pet click → open session (panel fallback lands in phase 3).

## Validation
- `cargo test`; node hook tests pass against the Rust server.
- macOS by eye with `BOBIX_HOME` = scratch home: install/uninstall produce the same `settings.json` as Electron for the same input (diff the files); Allow/Deny from a pet answers a real `hook.js` PermissionRequest; notification shows, click focuses the terminal; Allow/Deny buttons on the notification (unsigned build: report what macOS shows).
- Never run install against the real `~/.claude/settings.json` during dev.

## Risks / rollback
- Unsigned macOS builds may not show notification buttons (also true for Electron today; roadmap lists it as unverified).
- Rollback: revert the PR.
