# Phase 6: packaging, smoke tests, upgrade path

Back to [plan](plan.md). Still Electron for users: everything (Tauri installers only as CI artifacts).

## Context
- `package.json` `build`, `.github/workflows/{smoke,release}.yml`, `scripts/{make-icon.js,smoke-drag.mjs,record-demo.mjs}`, `docs/{install,development}.md`, `smoke()` in `main.js`.

## Requirements
- `tauri build` outputs with today's names: `Bobix-mac-arm64.dmg`, `Bobix-mac-x64.dmg`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb`, `bobix-linux-arm64.deb` (renamed in the workflow). Bundle id `dev.bobix.app`, product `Bobix`, `LSUIElement`, unsigned (`identity -`) as today, deb `StartupWMClass=bobix`, category Development, maintainer, version from one place.
- Smoke (`BOBIX_SMOKE=<dir>`): report.json with platform, visible, onTop, pets, petRects, pageErrors, real session via `hook.js`, chain/focus, pixel worker check (page-side), done file; screenshots by OS tools in the workflow. Same pass rule as today.
- Linux: installed `.deb` smoke on the Wayland path, and the drag test (rewritten without CDP).
- Upgrade path:
  - macOS: same bundle id and `Bobix.app` name → dmg drag replaces it; login item and notification permission stay.
  - Windows: NSIS pre-install hook finds the electron-builder install (`HKCU\…\Uninstall`, `Bobix`) and runs its uninstaller silently; on first start remove the old `Run` value if it points to a missing exe.
  - Linux: same package name `bobix`, higher version → `apt install ./bobix-linux-amd64.deb` replaces it; check `/opt/Bobix` is gone.
  - Settings, token, hooks, pet pictures read from the same places (phase 1).
- `scripts/record-demo.mjs` drives the Tauri build (or is marked Electron-only and removed in phase 7 if it can't).

## Files
- Create: `.github/workflows/tauri-release.yml` (workflow_dispatch only until phase 7), `src-tauri/windows/nsis-hooks.nsh`.
- Edit: `smoke.yml`, `scripts/smoke-drag.mjs` (→ `scripts/smoke-drag.sh` or Rust-reported), `src-tauri/tauri.conf.json`.

## Validation
- Smoke green on 3 OS for the Tauri build and still for Electron.
- macOS by eye: install Electron 0.1.4-rc dmg, set non-default settings and a pet picture, install the Tauri dmg over it: everything kept, hooks still deliver events (with a scratch HOME for hooks, never the real one without bobo's OK).
- Sizes: dmg, app, deb, exe; RAM of the full app (same sampler as the spike report).
- Windows/Linux upgrade: unverified unless run on real machines.

## Risks / rollback
- NSIS hook wrong → two installs; test in CI by installing the Electron exe then the Tauri exe on `windows-latest` and listing both folders.
- Rollback: revert the PR.
