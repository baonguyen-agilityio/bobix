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
npm test             # node tests (pages, pixel art, hook.js); not test/overlay.test.js, which needs Chrome
cd src-tauri && cargo test   # Rust tests (state, settings, hooks, server, Work tab, picture store...)
```

A copy run from source and an installed Bobix share the same port, settings and hooks. To run one next to the installed
app, give it its own: `BOBIX_PORT=4767 BOBIX_DATA_DIR=/tmp/bobix-data BOBIX_HOME=/tmp/bobix-home npm run tauri dev`
(`BOBIX_HOME` keeps hooks and start-at-login files out of your real home).

## Process

How a change gets made, by people and by Claude Code sessions alike. The working rules a session follows are in [`CLAUDE.md`](../CLAUDE.md).

1. **Issue.** Every task starts as a GitHub issue: the problem, what done looks like, and its edge cases (each one is checked before the merge).
2. **Prototype, when needed.** A new screen, a large layout change or a new area is first drawn as a prototype in a file **outside the repository**, with made-up data, and the owner approves it before the issue is written. Fixes and small changes need none. Prototypes, mockups and videos are never committed or put in a pull request.
3. **Worktree.** One git worktree per task, cut from `origin/develop` (`git fetch origin && git worktree add .claude/worktrees/<n>-<slug> -b <type>/<n>-<slug> origin/develop`). The main checkout stays on a clean `develop`. `.claude/worktrees/` is ignored by git. Remove the worktree and its test copies when the task ends.
4. **Pull request** to `develop` with "Closes #N". The [template](../.github/pull_request_template.md) is the definition of done: edge cases verified, tests with their output, tried in the real app with made-up data, EN and VI, docs and roadmap, CI green, every review comment answered.
5. **Review.** [`claude-review.yml`](../.github/workflows/claude-review.yml) runs two steps on every pull request from a branch of this repository, both on Opus at high effort, and comments on it: the code-review plugin, and a **docs check** that says where the README, the site, `docs/` (privacy, roadmap, install, development, work model), `CLAUDE.md` or an app string (EN and VI) no longer matches what the pull request changes. The docs check names the stale file and line in the comment, stays silent on a change nobody can see (a test, CI), and does not repeat a comment already on the pull request after a new push. Neither step approves, merges or blocks. Each comment is answered, one by one: fixed, or not an issue with the reason. It needs the repository secret `CLAUDE_CODE_OAUTH_TOKEN` (made by the owner with `claude setup-token`) and the Claude GitHub App; without the secret the job ends green with a notice, and it skips fork and bot (Dependabot) pull requests. Nothing but the owner handles the token.

   **The workflow file must be identical on `main` and `develop`.** The action refuses to run unless the file on the pull request's branch equals the one on `main`, so a pull request that edits it is always skipped; the job then ends green but shows a warning ("Claude review skipped") and a note in its summary, so a skipped review is not mistaken for a passed one. Land a change to this file as two pull requests with the same file: one to `develop`, one to `main`. The docs check is a prompt inside the workflow, not a skill file, so that one file is all `main` needs. One run per pull request at a time; each step has its own turn limit (25) and time limit (15 minutes), the job 30.
6. **CI.** The three smoke checks (see [CI](#ci)) must pass.
7. **The owner approves the merge.** Sessions never merge, and never `gh pr merge --auto`. A new feature is tried by hand by the owner first.

[`dependabot.yml`](../.github/dependabot.yml) opens one weekly pull request per ecosystem (cargo, npm, GitHub Actions). The smoke checks run on it; the review skips it. Read it like any other pull request before the owner merges it.

## Layout

| Path | What it is |
|---|---|
| `src-tauri/src/` | The app (Rust). `main.rs`: startup, the commands the pages call, the tests that keep their lists in step. `server.rs` and `state.rs`: local server, hook events, session state. `settings.rs`, `hooks.rs`, `paths.rs`: settings and the hooks installer. `overlay.rs`, `panel.rs`, `tray.rs`, `notify.rs`: the pets overlay, control panel, tray, notifications. `usage.rs`, `effects.rs`: usage per session and the effect of a change. `skills.rs`, `skill_audit.rs`, `habits.rs`, `habit_skills.rs`: skills audit and habits. `work.rs`, `tickets.rs`, `clones.rs`, `links.rs`, `model.rs`: Work tab, ticket flow and the [work model](work-model.md). `room.rs`: war room. `petstore.rs`: pet picture store. `smoke.rs`: the smoke check |
| `src-tauri/bridge.js` | `window.bobix` for the pages: the only way they reach the app |
| `src-tauri/build.rs`, `src-tauri/permissions/` | The commands the pages may call, declared to Tauri (`permissions/autogenerated/` is written by the build and committed) |
| `src-tauri/tauri.conf.json`, `Info.plist`, `windows/`, `icons/` | App and installer configuration |
| `src/ui/` | Pages: `pets.html` (overlay), `panel.html` (control panel), strings in `i18n.js` (the app reads them too), pet pictures as pixel art (`pixelate.js`, run in a Web Worker; hidden for now behind `PICTURE_PETS` in `creature.js`). The pages load only the files listed in `ASSETS` in `src-tauri/src/server.rs` |
| `src/ui/gif/` | GIF code copied unchanged from gifenc and gifuct-js (MIT, licenses next to it) |
| `src/hook/hook.js` | The forwarder Claude Code runs on each hook event (copied to `~/.claude/bobix/`) |
| `test/` | `core.test.js`: Node tests (`node --test`), run by `npm test`. `overlay.test.js` and `layout.test.js`: the pets overlay, and the panel with the pets card (no layout shift), in headless Chrome (CI runs them; skipped without Chrome); `chrome-helper.js` starts Chrome for both, with limits and cleanup. `fixtures/`: their input files |
| `docs/` | This documentation; `docs/media/` holds the screenshots and `demo.gif`, `docs/releases/` the release notes |
| `site/` | The project's web page (published by `.github/workflows/pages.yml`) |
| `.github/` | Workflows (`smoke.yml`, `release.yml`, `claude-review.yml`, `pages.yml`), `dependabot.yml`, the pull request template |
| `CLAUDE.md`, `.claude/settings.json` | The working rules and shared permissions for Claude Code sessions in this repository |

## Build installers

```bash
npm run tauri build   # installers land in src-tauri's build folder, under bundle/
```

Installers have fixed names (`Bobix-mac-arm64.dmg`, `Bobix-mac-x64.dmg`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb`, `bobix-linux-arm64.deb`), given by [the release workflow](../.github/workflows/release.yml), so `releases/latest/download/<name>` always serves the newest version. Keep the names there and the links in the READMEs and `docs/install.md` in step. Every push to `develop` builds them too: they are that run's artifacts on GitHub Actions, to try a build before it is released. Pull requests only run the smoke workflow; to try a PR's installers, start the release workflow by hand on its branch (Actions → release → Run workflow).

## Branches

- **`main`** is the latest stable release and the default branch. It does not take direct changes: it moves to a new version only when that version is released as stable.
- **`develop`** is where work lands. Open pull requests against `develop`; every push to it builds the installers as artifacts, and pre-releases (`v0.2.0-rc.N`) are tagged from it.
- **`v1`** carries maintenance of the v1 line (Electron, v0.1.x) from the commit `main` had before 0.2.0. A fix for v1 is a pull request to `v1`, shipped as a `v0.1.x` tag on `v1`. Fix the same bug on `develop` too if the code still exists there. **The v1 line ends with 0.2.0:** once 0.2.0 is released, `v1` gets no more fixes.

`develop`, `main` and `v1` are protected: changes come in by pull request only (also for administrators), with no force push and no deletion. On `develop` and `main` the three smoke checks (`tauri (macos-latest)`, `tauri (windows-latest)`, `tauri (ubuntu-22.04)`) must pass before a pull request can merge; the smoke workflow runs on every pull request, whatever files it changes, so a docs-only pull request can merge too. It also runs on pushes and on `v*` tags, and the release workflow on `v*` tags, so both work on any of the three branches. Tags are not protected. Repository settings (branch protection, Dependabot alerts, private vulnerability reporting, the review secret) are the owner's: a session does not change them.

## Release

1. Bump `version` in `package.json` (the app reads it from there) on a branch such as `chore/release-0.2.0` cut from the branch you release from (`develop`, or `v1` for a v0.1.x fix), and open a pull request to that branch. Those branches are protected, so the bump cannot be pushed straight to them: the smoke checks run on the pull request and the owner merges it.
2. When it has merged, tag the merge commit: `git fetch origin && git tag v0.2.0 origin/develop && git push origin v0.2.0` (`origin/v1` for a v0.1.x fix). Tags are not protected. The release workflow builds on macOS, Windows and Ubuntu and attaches the installers to a GitHub Release. A tag with a dash (`v0.2.0-rc.1`) makes a pre-release, which does not become "latest".
3. Add release notes to the release.
4. For a release with changes people should know about, rewrite the tour's steps (`STEPS` in `src/ui/tour.js`, each pointing at a real element, with its `tour.<key>.title` / `.body` strings in EN and VI), mark the one about this release `news: true`, and set `NEWS` in `src/ui/i18n.js` before tagging: the panel shows everyone updating to that version that one step once, as a single bubble (Done marks every step seen), never a new install; Setup → "Show the tour again" shows the whole tour. What a group or a setting does goes on its "?" instead (`HELP` in `src/ui/tour.js`, strings `help.grp.<key>` / `help.row.<key>`).

## CI

[The smoke workflow](../.github/workflows/smoke.yml) runs on every push: node and Rust tests, then the app on each OS with a pretend and a real session (`BOBIX_SMOKE`, see `src-tauri/src/smoke.rs`): pets on screen, a held permission request answered, the panel and its bridge, a pet picture imported, the Skills screen's audit actions on a scratch skill and the session's `CLAUDE.md` (each undone), the tray, start at login. On Linux it takes the Wayland path (XWayland) and drags a pet with the real mouse (`xdotool`), then points at a pet: the line of pets must stop. To run the same check on your machine before pushing (scratch folders, so it never touches your own settings or hooks):

```bash
cd src-tauri && cargo build && cd .. && T=$(mktemp -d) && BOBIX_SMOKE=$T/out BOBIX_PORT=4767 BOBIX_DATA_DIR=$T/data BOBIX_HOME=$T/home ./src-tauri/target/debug/bobix; cat $T/out/report.json
```

It exits by itself and `report.json` must say `"pass": true` with empty `pageErrors`. Started from inside a Claude Code session, unset the `CLAUDE*` variables first (`CLAUDE_PID`, `CLAUDE_CODE_HOST_SESSION_ID`): the pretend session would otherwise belong to that session and wait for its parent link, and the check counts one pet too few. The pages load only the files `src-tauri/src/server.rs` lists in `ASSETS` (a new file under `src/ui/` must be added there; `npm test` checks it).

[The release workflow](../.github/workflows/release.yml) builds the installers, installs each (over the last Electron-based release on Windows and Linux) and runs the same check on the installed app.

## Demo and screenshots

The screenshots and `demo.gif` in `docs/media/` were taken from a debug build in a scratch world (its own port, data folder and `BOBIX_HOME`, made-up repositories and transcripts), capturing only Bobix's own windows. Keep them few and small (about 0.7 MB in all), and take new ones when the pages change. The app icon comes from one 1024 px picture: `npx tauri icon <png>` writes `src-tauri/icons/`; the tray icon is drawn in `src-tauri/src/tray.rs`.
