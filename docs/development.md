# Developing Bobix

For contributors. To **use** Bobix, install it from the [latest release](https://github.com/baonguyen-agilityio/bobix/releases/latest) as the [README](../README.md#install) shows; don't build it from source for that.

## Run and test

Requires Node.js 22+.

```bash
git clone https://github.com/baonguyen-agilityio/bobix.git && cd bobix
npm install
npm start            # run the app from source
npm test             # unit tests
```

A copy run from source and an installed Bobix share the same port and lock: quit one before starting the other.

## Layout

| Path | What it is |
|---|---|
| `src/main.js` | Electron main process: server, pets overlay, control panel, tray, notifications |
| `src/core/` | State, settings, hooks installer, window focus, local server |
| `src/hook/hook.js` | The forwarder Claude Code runs on each hook event (copied to `~/.claude/bobix/`) |
| `src/ui/` | Pages: `pets.html` (overlay), `panel.html` (control panel), strings in `i18n.js` |
| `test/core.test.js` | Unit tests (`node --test`) |
| `scripts/` | Icon and demo generators, CI checks |

## Build installers

```bash
npm run dist:mac     # or dist:win, dist:linux; installers land in the dist folder
```

Installers have fixed names (`Bobix-mac-arm64.dmg`, `Bobix-mac-x64.dmg`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb`, `bobix-linux-arm64.deb`), so `releases/latest/download/<name>` always serves the newest version. Keep the names in `package.json` (`artifactName`) and the links in the READMEs and `docs/install.md` in step.

## Release

1. Bump `version` in `package.json` and commit on `main`.
2. Push a tag such as `v0.1.2`. [The release workflow](../.github/workflows/release.yml) builds on macOS, Windows and Ubuntu and attaches the installers to a GitHub Release. A tag with a dash (`v0.2.0-rc.1`) makes a pre-release, which does not become "latest".
3. Add release notes to the release.

## CI

[The smoke workflow](../.github/workflows/smoke.yml) runs on every push: unit tests, then the real app on each OS with a pretend session. On Linux it also installs the `.deb`, takes the Wayland path (restart through XWayland), and drags a pet with the real mouse (`scripts/smoke-drag.mjs`).

## Demo and screenshots

`node scripts/record-demo.mjs` re-records `docs/media/demo.mp4`, the GIF and the screenshots from a separate app instance with pretend sessions.
