# Phase 5: Linux and Windows details

Back to [plan](plan.md). Still Electron for users: everything.

## Context
- `main.js` top (WAYLAND / OZONE / SHAPED / XWAYLAND), `shapeOverlay`, `trackMouse`, `showOverlay` (always-on-top re-set on Linux), `focus.js` Linux/Windows branches, `hasXdotool`.

## Requirements
- Linux on Wayland with `DISPLAY` set: `GDK_BACKEND=x11` before GTK starts (XWayland) unless `BOBIX_WAYLAND=1`; no restart needed.
- Linux X11 / XWayland: per-pet input shape (pets' rects + 16 px pad) through GTK `input_shape_combine_region`; no cursor-based ignore toggling there; on XWayland the page uses its own mouse events (no cursor poll), as today.
- Always-on-top and all-workspaces set again every time the overlay shows (Linux drops them).
- Focus on Linux needs `xdotool`; reasons `wayland` / `xdotool` as today. Windows: PowerShell `AppActivate`, `tasklist` names.
- Windows notifications show the app name (AppUserModelID of the installed app).
- Diagnostics: `session` type, wayland flag, backend.

## Files
- Edit: `src-tauri/src/overlay.rs`, `main.rs`, `focus.rs`, `notify.rs`, `diagnostics.rs`; Linux-only deps (`gtk`, `gdk`, `cairo` through Tauri's re-exports).

## Validation
- CI Ubuntu: Xvfb with `XDG_SESSION_TYPE=wayland` (as today), overlay on top, pets ≥ 2, then `xdotool` drag of a pet using rects from the smoke report; page reports the new position.
- CI Windows: smoke green.
- By eye: macOS regression check only. **Real Ubuntu desktop and real Windows: unverified unless someone runs the CI artifacts** (report says so).

## Risks / rollback
- WebKitGTK transparent window on X11 without a compositor shows black (same for Electron); documented.
- Rollback: revert the PR.
