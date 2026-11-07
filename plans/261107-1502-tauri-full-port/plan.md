# Port all of Bobix from Electron to Tauri v2

Status: planned, waiting for bobo's OK (2026-11-07). Branch `feat/tauri-port` from `origin/develop` once #41, #42 and `spike/tauri` are in it; one PR per phase into `develop`, never `main`.

## Goal

At the end, Bobix is a Tauri v2 app: a Rust core plus the existing HTML/JS pages (`src/ui/`). No Electron left in the repo, the installers or CI. Every feature works as it does today, and existing users upgrade in place: same settings file, same token, same hooks in `~/.claude/settings.json`, same installer names.

Not in scope: anything new. No new features, settings or UI changes beyond what the port forces (listed under "Known differences").

## How the two apps live side by side until the end

- Phases 1–6 add a Tauri app in `src-tauri/` next to the Electron app. **Users keep getting the Electron app** until phase 7: `npm start`, the release workflow and the installers stay Electron.
- The Tauri app is built and smoke-tested in CI on every PR, and can be tried from CI artifacts. In dev it runs on its own port and data folder (`BOBIX_PORT`, `BOBIX_DATA_DIR`, `BOBIX_HOME` for hooks), so it never touches the running Bobix or the real `~/.claude/settings.json`.
- Phase 7 switches release + CI to Tauri and deletes Electron (`src/main.js`, `src/preload.js`, `src/core/*.js` that moved to Rust, `electron`, `electron-builder`).

## Architecture (Tauri)

- **Same shape as today.** Rust starts the 127.0.0.1 server (axum on Tauri's tokio runtime) with the token, `/event`, `/events` (SSE), `/pet-image/…` and the pages. The pages are embedded in the binary (Tauri assets from `src/ui/`) and served by that server, so `pets.html` and `panel.html` keep loading from `http://127.0.0.1:<port>/` and keep their `EventSource('/events?t=…')` unchanged.
- **`window.bobix` stays the only bridge.** `src/preload.js` becomes an init script (`src-tauri/bridge.js`) that maps every call to a Tauri command and every `on…` event to a function Rust calls with `eval`. Commands from our http origin are allowed through an app manifest (`build.rs`) and a capability added at runtime for exactly `http://127.0.0.1:<port>/*` and the `pets` / `panel` windows (the spike's IPC/ACL lesson). The pages need no changes.
- **Pet pictures stay JavaScript.** `pixelate.js` + the GIF code run in a Web Worker in the panel (the only place an import happens). PNG/JPG/WebP are decoded by the panel with `createImageBitmap` + canvas (this also removes the hidden-page WebP trick). Rust only opens the file dialog, checks size and type, and stores files (`petstore`). Reason: 1,400 lines of tested pixel/GIF code keep working as they are; a Rust rewrite would change pictures and tests for no user gain.
- **`hook.js` is unchanged** (embedded, copied to `~/.claude/bobix/hook.js` as today). The curl / PowerShell fallbacks are unchanged.

## Phases

| # | Phase | Delivers (Tauri app) | Still Electron for users | Days |
|---|---|---|---|---|
| 1 | [Core and pets](phase-01-core-and-pets.md) | Rust server + token + SSE, session state, transcript reading, settings file, pets overlay with click-through (macOS, Windows), timers, test event | everything | 5–7 |
| 2 | [Hooks, answers, notifications, open session](phase-02-hooks-answers-notifications.md) | hooks install/uninstall/status, Allow/Deny, session titles for issues, notifications (+ macOS Allow/Deny buttons), open/focus session per OS | everything | 4–6 |
| 3 | [Panel, tray, settings, work tab](phase-03-panel-tray-settings-work.md) | panel (E layout) with every bridge call, tray, settings changes, i18n, autostart, single instance, update check, feedback/diagnostics, Work tab (git, `glab`, New/Resume with `[KEY-iid]` names) | everything | 6–9 |
| 4 | [Pet pictures](phase-04-pet-pictures.md) | Change pet / Edit picture / Default pet, pixel art in the panel's worker, stored files | everything | 2–3 |
| 5 | [Linux and Windows details](phase-05-linux-windows.md) | Linux: XWayland, per-pet input shape, xdotool focus; Windows: focus, notifications | everything | 3–5 |
| 6 | [Packaging, smoke tests, upgrade path](phase-06-packaging-smoke-upgrade.md) | installers with today's names for 3 OS, smoke tests rewritten for Tauri, upgrade from the Electron install checked | everything (Tauri installers only as CI artifacts) | 3–5 |
| 7 | [Switch and remove Electron](phase-07-switch-remove-electron.md) | release workflow and CI on Tauri only, Electron code and packages deleted, docs updated | **nothing: Electron removed** | 2–3 |

Total 25–38 dev-days. Each phase: PR into `develop`, CI green on macOS, Windows and Ubuntu, checked by eye on macOS, report to bobo. Windows and Linux behaviour not seen by eye is reported as unverified. No tag or release without bobo's OK.

## Electron → Tauri, per feature

| Feature | Electron today | Tauri | Phase |
|---|---|---|---|
| Local server, token, SSE | `core/server.js` (node http) | `server.rs` (axum), same routes, Host check, timing-safe token compare, 8 MB cap, 25 s ping | 1 |
| Session state, transcript | `core/state.js` | `state.rs` (same rules: `apply`, `plain`, `clip`, `readTranscript`, branches, `gone`) | 1 |
| Project of a session, side-session parent | `core/project.js`, `core/links.js` | `project.rs`, `links.rs` | 1 |
| Settings file | `core/settings.js`, `userData/settings.json` | `settings.rs`; same path (`~/Library/Application Support/Bobix`, `%APPDATA%\Bobix`, `~/.config/Bobix`), same keys and checks, atomic write | 1 |
| Token | `~/.claude/bobix/token`, `header` | same files, same format | 1 |
| Pets overlay | `BrowserWindow` transparent, `setIgnoreMouseEvents`, cursor poll 33 ms | `WebviewWindow` transparent, `set_ignore_cursor_events`, same poll (spike code); macOS full-screen Spaces via NSWindow collection behaviour | 1 |
| Hooks install/uninstall/status | `core/hooks.js` | `hooks.rs`, same JSON entries, backups, symlink + mode kept, outdated/stale rules; `findBin` via login shell | 2 |
| Permission Allow / Deny | held `/event` reply, `answer()` | same: held reply, `x-bobix-wait` header, abort on close | 2 |
| Session title from issue (#41) | SessionStart `output` | same | 2 |
| Notifications | Electron `Notification` (+ macOS action buttons, click opens session) | `notify-rust` / `mac-notification-sys` directly (the notification plugin has no desktop click/actions; to confirm) | 2 |
| Open / focus session | `core/focus.js` (`ps`, `open -a`, PowerShell, `xdotool`), `claude://` links | `focus.rs` (same commands), links through `tauri-plugin-opener` | 2 |
| Panel window | `BrowserWindow` 1180×780, blocked navigation, Dock shown while open | `WebviewWindow`, `on_navigation` guard, `set_activation_policy` | 3 |
| Bridge (`window.bobix`) | `preload.js` + `ipcMain` | `bridge.js` init script + Tauri commands; events by `eval` | 1 (pets), 3 (panel) |
| Tray | `Tray` + `Menu` | Tauri tray + menu (checkbox item for Pause), icon from `icon.rs` | 3 |
| Settings changes | `updateSettings()` (port move + relaunch, display/edge/size, autostart…) | same in Rust; `app.restart()` | 3 |
| i18n | `ui/i18n.js` (pages), `I18N.t` in main | pages unchanged; the ~25 strings Rust needs in `i18n.rs`, a node test keeps them equal to `i18n.js` | 3 |
| Autostart | `core/autostart.js`, `setLoginItemSettings` | `autostart.rs`: Linux same `.desktop` file, Windows `Run` registry key, macOS SMAppService (same as Electron's login item) | 3 |
| Single instance + "another version" notice | `requestSingleInstanceLock(ME)` | `tauri-plugin-single-instance`; the new copy first posts its version to the running one's server (token) | 3 |
| Update check | `core/update.js` (fetch) | `update.rs` (`ureq`) | 3 |
| Feedback, diagnostics, clipboard, reveal settings | `shell`, `clipboard` | `feedback.rs`; opener + clipboard plugins | 3 |
| Work tab: git status, GitLab via `glab`, New/Resume, worktrees | `core/work.js`, `startIssue` | `work.rs` (same commands, argument lists, no shell) | 3 |
| Pet pictures | `pixelate.js` in a worker thread, `nativeImage`, hidden page for WebP | `pixelate.js` in a panel Web Worker, decode in the panel; `petstore.rs`, dialog plugin | 4 |
| Linux Wayland → XWayland | restart with `--ozone-platform=x11` | `GDK_BACKEND=x11` before GTK starts (no restart); `BOBIX_WAYLAND=1` keeps Wayland | 5 |
| Linux click-through | `setShape` around pets | GTK `input_shape_combine_region` around pets | 5 |
| Packaging | electron-builder dmg/nsis/deb | `tauri build` dmg/nsis/deb, same file names, same bundle id `dev.bobix.app` | 6 |
| Smoke tests | `BOBIX_SMOKE`, `executeJavaScript`, `desktopCapturer`, CDP drag | `BOBIX_SMOKE` in Rust, page reports through a smoke-only command, screenshot by OS tools in CI, drag via `xdotool` + page report | 6 |
| Unit tests | `node --test test/core.test.js` | `cargo test` for Rust modules (same cases), node tests kept for `pixelate`, `markdown`, `log`, `i18n`, `hook.js` | each phase |

## Risks

| Risk | Impact | Plan |
|---|---|---|
| Linux click-through has no Tauri API | pets block clicks or can't be clicked on Linux | GTK input shape (phase 5); CI drag test with `xdotool`; real Ubuntu desktop by eye **unverified** unless someone runs it |
| Notification click + Allow/Deny buttons | answer from a notification lost | `mac-notification-sys` (macOS), `notify-rust` (Linux), WinRT toast (Windows); check by eye on macOS; Windows/Linux unverified |
| Settings folder name differs (`Bobix` vs `bobix`; Linux is case-sensitive) | upgraded Linux users lose settings | phase 1: print Electron's `userData` in CI on Linux; read both names, prefer the existing one |
| Windows: Tauri NSIS installs to a different folder than electron-builder | two Bobix installs side by side after upgrade | phase 6: NSIS pre-install hook runs the old uninstaller silently; old autostart entry removed on first start; **needs a real Windows run** |
| Display id: Tauri monitors have no Electron display id | a chosen non-primary display falls back to primary once | match by name; stored numeric ids map on macOS via CGDirectDisplayID if cheap, else documented one-time reset |
| No display-change event in Tauri | overlay not moved when a monitor is plugged | poll monitors every 5 s (cheap) |
| macOS full-screen Spaces (`visibleOnFullScreen`) | pets missing over full-screen apps | NSWindow collection behaviour via objc; check by eye |
| WebView throttling of the never-focused overlay | pets freeze | spike showed none on macOS; smoke checks pets move; Windows/Linux unverified |
| Linux deb needs `libwebkit2gtk-4.1` | no install on Ubuntu 20.04 / Debian 11 | documented minimum (Ubuntu 22.04+, Debian 12+) in `docs/install.md` |
| Windows needs the WebView2 runtime | first start needs a download on old Windows 10 | NSIS `downloadBootstrapper` (default) |
| Bundle id ends in `.app` (Tauri warns) | none at run time | keep `dev.bobix.app`: notification permission and login item stay with the app |
| Disk on this Mac (~9 GB free) | builds fail | one `CARGO_TARGET_DIR` in the scratchpad, `cargo clean -p` old builds, delete `tauri-spike/` target |

## Known differences (forced by the port)

- Electron-only flags such as `--no-sandbox` / `--ozone-platform` stop meaning anything; `BOBIX_WAYLAND=1` still keeps Wayland.
- "Copy diagnostics" reports `webview` instead of `ozone`; the feedback text changes the same way.

## Acceptance (whole port)

- Every row of the mapping table works in the Tauri build; `cargo test` and the kept node tests pass; smoke green on 3 OS.
- An Electron install upgraded with the Tauri installer keeps settings, token, hooks, pet pictures and start-at-login (macOS by eye; Windows/Linux reported unverified unless run on a real machine).
- `grep -ri electron` finds nothing but history (changelog / roadmap notes).
- Sizes measured again (dmg, app, RAM) and noted in the last phase's report.

## Unresolved questions

- Is there a real Windows and a real Ubuntu desktop to check phases 5–6 by eye? Without one they ship as "CI only".
- Version number for the first Tauri pre-release (suggest `0.2.0-rc.1`)? Tag only with bobo's OK.
