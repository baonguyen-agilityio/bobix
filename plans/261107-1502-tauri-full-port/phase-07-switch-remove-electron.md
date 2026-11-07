# Phase 7: switch to Tauri and remove Electron

Back to [plan](plan.md). After this phase nothing runs on Electron.

## Requirements
- Release workflow builds only the Tauri installers (tag push, pre-release on `-` tags as today); `tauri-release.yml` becomes `release.yml`.
- Smoke workflow: Tauri only.
- Delete: `src/main.js`, `src/preload.js`, `src/core/*.js` and `test/` cases that moved to Rust, `electron`, `electron-builder`, the `build` block in `package.json`, Electron-only scripts. `package.json` keeps only what the node tests and scripts need (`npm test`, `npm run tauri`).
- Docs: README (EN, VI) install notes (Ubuntu 22.04+ / Debian 12+, WebView2), `docs/development.md` (Rust toolchain, `npm run tauri dev`, layout table), `docs/install.md`, `docs/privacy.md` (nothing changes in what is sent; check wording), `docs/roadmap.md` (port done, what is unverified).
- Version bump for the first Tauri pre-release (number chosen by bobo); **no tag without bobo's OK**.

## Validation
- `grep -rin electron` in the repo: only history notes.
- CI green on 3 OS; macOS by eye: install from the CI dmg, full walk through pets, panel, tray, notifications, Allow/Deny, Work tab, pet pictures, settings, start at login, update check.
- Report: final sizes and RAM vs the Electron baseline, list of what is unverified on Windows/Linux.

## Rollback
- Revert the PR: Electron comes back as it was in phase 6 (both apps still in the tree there).
