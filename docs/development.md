# Developing Bobix

What is missing, unreleased or unverified: [roadmap.md](roadmap.md).

For contributors. To **use** Bobix, install it from the [latest release](https://github.com/baonguyen-agilityio/bobix/releases/latest) as the [README](../README.md#install) shows; don't build it from source for that.

## Run and test

Requires Node.js 22+ and Rust (stable, via [rustup](https://rustup.rs)). On Linux also the WebKitGTK and tray libraries:
`sudo apt install libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev`.

```bash
git clone https://github.com/baonguyen-agilityio/bobix.git && cd bobix
npm install
npm run tauri dev    # run the app from source
npm test             # node tests (pages, pixel art, hook.js)
cd src-tauri && cargo test   # Rust tests (state, settings, hooks, server, Work tab, picture store...)
```

A copy run from source and an installed Bobix share the same port, settings and hooks. To run one next to the installed
app, give it its own: `BOBIX_PORT=4767 BOBIX_DATA_DIR=/tmp/bobix-data BOBIX_HOME=/tmp/bobix-home npm run tauri dev`
(`BOBIX_HOME` keeps hooks and start-at-login files out of your real home).

## Layout

| Path | What it is |
|---|---|
| `src-tauri/src/` | The app (Rust): local server and hook events (`server.rs`, `state.rs`), settings, hooks installer, pets overlay, control panel, tray, notifications, Work tab, the [work model](work-model.md) (`model.rs`), pet picture store, smoke check |
| `src-tauri/bridge.js` | `window.bobix` for the pages: the only way they reach the app |
| `src/ui/` | Pages: `pets.html` (overlay), `panel.html` (control panel), strings in `i18n.js` (the app reads them too), pet pictures as pixel art (`pixelate.js`, run in a Web Worker; hidden for now behind `PICTURE_PETS` in `creature.js`) |
| `src/ui/gif/` | GIF code copied unchanged from gifenc and gifuct-js (MIT, licenses next to it) |
| `src/hook/hook.js` | The forwarder Claude Code runs on each hook event (copied to `~/.claude/bobix/`) |
| `test/core.test.js` | Node tests (`node --test`) |

## Build installers

```bash
npm run tauri build   # installers land in src-tauri's build folder, under bundle/
```

Installers have fixed names (`Bobix-mac-arm64.dmg`, `Bobix-mac-x64.dmg`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb`, `bobix-linux-arm64.deb`), given by [the release workflow](../.github/workflows/release.yml), so `releases/latest/download/<name>` always serves the newest version. Keep the names there and the links in the READMEs and `docs/install.md` in step. Every push to `develop` builds them too: they are that run's artifacts on GitHub Actions, to try a build before it is released. Pull requests only run the smoke workflow; to try a PR's installers, start the release workflow by hand on its branch (Actions → release → Run workflow).

## Release

1. Bump `version` in `package.json` (the app reads it from there) and commit on `main`.
2. Push a tag such as `v0.2.0`. The release workflow builds on macOS, Windows and Ubuntu and attaches the installers to a GitHub Release. A tag with a dash (`v0.2.0-rc.1`) makes a pre-release, which does not become "latest".
3. Add release notes to the release.
4. For a release with changes people should know about, rewrite the tour's steps (`STEPS` in `src/ui/tour.js`, each pointing at a real element, with its `tour.<key>.title` / `.body` strings in EN and VI) and `NEWS` in `src/ui/i18n.js` before tagging: the panel walks everyone updating to that version through it once (Skip or Done marks it seen), never a new install; Setup → "Show the tour again" shows it again.

## CI

[The smoke workflow](../.github/workflows/smoke.yml) runs on every push: node and Rust tests, then the app on each OS with a pretend and a real session (`BOBIX_SMOKE`, see `src-tauri/src/smoke.rs`): pets on screen, a held permission request answered, the panel and its bridge, a pet picture imported, the Skills screen's audit actions on a scratch skill and the session's `CLAUDE.md` (each undone), the tray, start at login. On Linux it takes the Wayland path (XWayland) and drags a pet with the real mouse (`xdotool`), then points at a pet: the line of pets must stop. To run the same check on your machine before pushing (scratch folders, so it never touches your own settings or hooks):

```bash
cd src-tauri && cargo build && cd .. && T=$(mktemp -d) && BOBIX_SMOKE=$T/out BOBIX_PORT=4767 BOBIX_DATA_DIR=$T/data BOBIX_HOME=$T/home ./src-tauri/target/debug/bobix; cat $T/out/report.json
```

It exits by itself and `report.json` must say `"pass": true` with empty `pageErrors`. Started from inside a Claude Code session, unset the `CLAUDE*` variables first (`CLAUDE_PID`, `CLAUDE_CODE_HOST_SESSION_ID`): the pretend session would otherwise belong to that session and wait for its parent link, and the check counts one pet too few. The pages load only the files `src-tauri/src/server.rs` lists in `ASSETS` (a new file under `src/ui/` must be added there; `npm test` checks it).

[The release workflow](../.github/workflows/release.yml) builds the installers, installs each (over the last Electron-based release on Windows and Linux) and runs the same check on the installed app.

## Demo and screenshots

The media in `docs/media/` were recorded from the 0.1 app; the recording script went with it. Record new ones by hand when the pages change.
