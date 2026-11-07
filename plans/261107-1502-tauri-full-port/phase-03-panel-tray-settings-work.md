# Phase 3: panel, tray, settings, Work tab

Back to [plan](plan.md). Still Electron for users: everything.

## Context
- `main.js`: `showPanel`, `broadcastSettings`, `buildTray`, `updateSettings`, `portFree`, `ipc()`, `diagnostics`, `feedbackInfo`/`sendFeedback`, `checkForUpdate`, `migrateFromOldName`, `second-instance`, boot order, `refreshGit`, `refreshIssues`, `workView`, `startIssue`, `openIssue`, `titleFor`.
- `core/{autostart,update,feedback,news,work,icon}.js`, `ui/i18n.js`, `ui/panel.html` (E layout after #42).

## Requirements
- Panel window: 1180×780, min 420×480, title Bobix, background `#f3f4f8`, no menu, navigation and new windows blocked, opened with `?s=`, `tab=`, `fail=`; `select` / `show-tab` / `open-fail` events when already open; Dock icon shown while open on macOS, hidden on close.
- Full bridge: every call in `preload.js` (getState, setSettings, resetSettings, revealSettings, copyDiagnostics, feedbackInfo, sendFeedback, installHooks, uninstallHooks, testEvent, openSession, answer, petImage (phase 4), setPet, refreshWork, openIssue, startIssue, onSettings, onSelect, onShowTab, onOpenFail).
- Settings: `updateSettings` rules (port: free check, re-install hooks, save, restart; display/edge/size → move overlay; petsVisible; tray rebuild; releaseUnseen; GitLab refresh; update check toggles), 300 ms save debounce, reset keeps the same keys as today. Display list and work area from Tauri monitors; monitors polled every 5 s for changes.
- Tray: same items and order, version line, Pause checkbox, What's new item, update item; 32 px creature icon.
- i18n: `language: auto` from the system locale; Rust strings in `i18n.rs`; a node test checks they equal `i18n.js`.
- Autostart: same behaviour as `autostart.js` per OS; the boot rule about clones kept.
- Single instance: second start shows the panel; a different version shows the "other version" notification.
- Update check: 15 s after start, then daily; one notification per version; tray item.
- Diagnostics (to clipboard) and feedback link: same fields, redaction unchanged (`feedback.rs` + its tests).
- Work tab: git status per folder (30 s, forced on Stop/SessionStart), `glab api` issues every 5 min while on, issue per session (branches, parent), `[KEY-iid]` / `<project> #<iid>` session titles as #41 defines, New (worktree + `claude://code/new?folder=`) and Resume, only configured-GitLab links open. Remotes never sent to pages.
- Old name migration (`Session Guild` settings and autostart files).
- First run: panel opens on Setup when no hooks are installed.

## Files
- Create: `src-tauri/src/{panel.rs,tray.rs,i18n.rs,autostart.rs,update.rs,feedback.rs,work.rs,diagnostics.rs,instance.rs,icon.rs,migrate.rs}`.
- Edit: `bridge.js`, `main.rs`, `settings.rs`, `test/` (i18n parity test).

## Validation
- `cargo test` with ported cases for work (parseStatus, projectPath, issueOfBranch, issueOfSession, branchFor, worktreesOf, sessionTitleFor, groupIssues), update, feedback, news, autostart paths.
- macOS by eye: every panel tab (Sessions board E, Work, Setup, Settings, Feedback) against the Electron panel side by side; tray items; port change restarts on the new port; second start; Work tab against the real self-hosted GitLab through bobo's own `glab` (read-only); New creates a worktree in a scratch repo, not a real project.

## Risks / rollback
- Panel parity is the biggest UI surface: compare screenshots tab by tab with Electron before the PR.
- Rollback: revert the PR.
