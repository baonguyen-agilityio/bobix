# Phase 1: Rust core and pets overlay

Back to [plan](plan.md). Still Electron for users: everything.

## Context
- `src/main.js` (`onEvent`, timers, `createOverlay`, `trackMouse`, `overlayBounds`, `testEvent`), `src/core/server.js`, `state.js`, `settings.js`, `project.js`, `links.js`, `hooks.js` (`ensureToken` only), `src/preload.js`.
- Spike: `tauri-spike/src-tauri/src/main.rs` (overlay window, cursor poll, ACL in `build.rs` + capability).

## Requirements
- `src-tauri/` at the repo root replaces `tauri-spike/` (deleted with its workflow). Tauri 2, release profile as in the spike.
- Server on `127.0.0.1:<settings.port>`: `POST /event` (token header, Host check, 8 MB cap, UTF-8 body), `GET /events` SSE (token in `?t=`, snapshot first, 25 s ping), `GET /`, `/pets`, `/ui/<asset>` and `/ui/fonts/…` from embedded `src/ui/` with today's allow-list and `no-store`; 403/404/405 as today. `/pet-image/` arrives in phase 4.
- State: `apply`, `refresh`/`readTranscript` (512 KB tail, head scan for titles, branches, interrupted), `ctxLeft`, `clip`, `plain`, `short`, system-prompt tags, `pinProject`, `publicView`, `gone`, `setPet`, `findParent`; Stop handled after 300 ms; 5 s timer (stale 12 h, gone pids, transcript refresh, parent links).
- Settings: same defaults, `sanitize`, `holdSecs`, load/save (tmp + rename) at Electron's `userData/settings.json`. `BOBIX_DATA_DIR` and `BOBIX_PORT` override for dev.
- Token: `ensureToken` (same files, modes 0700/0600).
- Snapshot sent to pages is the same JSON as `snapshot()` today (`work` stays `{status:'off'}` until phase 3, `image` null until phase 4).
- Pets overlay: same bounds rule, hidden while no rects, always on top, all workspaces incl. full-screen Spaces on macOS, no Dock icon, not focusable, click-through except over pets (33 ms poll), cursor fed to the page.
- Bridge for the pets page: `getState`, `onSettings`, `onMouse`, `setRects`, `petClick` (opens the panel later; logs for now), `setPet`, `answer` (wired in phase 2).
- Test event (`hooks:test` equivalent, used by smoke): start → prompt → tool → stop → end, same timings.

## Files
- Create: `src-tauri/{Cargo.toml,build.rs,tauri.conf.json,capabilities/,icons/}`, `src-tauri/src/{main.rs,server.rs,state.rs,settings.rs,project.rs,links.rs,token.rs,overlay.rs}`, `src-tauri/bridge.js`.
- Delete: `tauri-spike/`, `.github/workflows/tauri-spike.yml`.
- Edit: `.github/workflows/smoke.yml` (add a `tauri` job: `cargo test`, `tauri build --debug --no-bundle`, run with `BOBIX_SMOKE`), `.gitignore`.

## Steps
1. Move spike code into `src-tauri/`; add axum; embed `src/ui` via `frontendDist`.
2. Port `settings.rs`, `token.rs`, `project.rs`, `links.rs`, `state.rs` with their tests from `test/core.test.js` (same cases, same fixtures).
3. `server.rs` with the tests of "server: header + host guard…".
4. Overlay + bridge; runtime capability for `http://127.0.0.1:<port>/*`.
5. Minimal `BOBIX_SMOKE`: test event + real `hook.js` runs (with `node`), page reports pets count/rects through a smoke-only command, exits 0/1. CI on 3 OS.
6. CI step on Linux prints Electron's `userData` path to settle `Bobix` vs `bobix`.

## Validation
- `cargo test` green; `npm test` still green (Electron untouched).
- macOS by eye: dev run on port 4767 with `BOBIX_DATA_DIR` in the scratchpad (Bobix `settings.json` backed up first anyway); test event shows a walking pet; click-through outside pets; hook.js fed events show a session; SSE in a browser tab with the token.
- Smoke green on macOS, Windows, Ubuntu (Linux click-through not expected yet).

## Risks / rollback
- Remote-origin IPC denied silently → capability tested in smoke (rects must arrive).
- Rollback: revert the PR; Electron untouched.
