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

[`dependabot.yml`](../.github/dependabot.yml) opens one weekly pull request per ecosystem and directory (cargo for `src-tauri`, `crates/bobix-link` and its `fuzz` crate, and `relay`; npm; GitHub Actions). The smoke checks run on it; the review skips it. Read it like any other pull request before the owner merges it.

## Layout

| Path | What it is |
|---|---|
| `src-tauri/src/` | The app (Rust). `main.rs`: startup, the commands the pages call, the tests that keep their lists in step. `server.rs` and `state.rs`: local server, hook events, session state. `settings.rs`, `hooks.rs`, `paths.rs`: settings and the hooks installer. `overlay.rs`, `panel.rs`, `tray.rs`, `notify.rs`: the pets overlay, control panel, tray, notifications. `usage.rs`, `effects.rs`: usage per session and the effect of a change. `skills.rs`, `skill_audit.rs`, `habits.rs`, `habit_skills.rs`: skills audit and habits. `work.rs`, `tickets.rs`, `clones.rs`, `links.rs`, `model.rs`: Work tab, ticket flow and the [work model](work-model.md). `room.rs`: war room. `petstore.rs`: pet picture store. `phones.rs`, `link.rs`, `phone_view.rs`, `presence.rs`, `secrets.rs`: phone access (the paired phones and their commands, the relay connection, what a phone sees, the input time, the keychain); the shared wire format is `crates/bobix-link`. `smoke.rs`, `smoke_phones.rs`: the smoke check (with `BOBIX_SMOKE_PHONES`, `fake-phone` against a relay) |
| `src-tauri/bridge.js` | `window.bobix` for the pages: the only way they reach the app |
| `src-tauri/build.rs`, `src-tauri/permissions/` | The commands the pages may call, declared to Tauri (`permissions/autogenerated/` is written by the build and committed) |
| `src-tauri/tauri.conf.json`, `Info.plist`, `windows/`, `icons/` | App and installer configuration |
| `src/ui/` | Pages: `pets.html` (overlay), `panel.html` (control panel), strings in `i18n.js` (the app reads them too), pet pictures as pixel art (`pixelate.js`, run in a Web Worker; hidden for now behind `PICTURE_PETS` in `creature.js`). The pages load only the files listed in `ASSETS` in `src-tauri/src/server.rs` |
| `mobile/` | The phone app (Flutter, Riverpod, app id `dev.bobix.phone`, iOS 18 and Android 10 or later). Its strings are the `phone.*` keys of `src/ui/i18n.js`: `node mobile/tool/strings.mjs` writes `mobile/lib/l10n/strings.g.dart` and the iOS `InfoPlist.strings` files. Its pets are `lib/pets/creature.dart`, a port of `creature.js` checked against `mobile/test/fixtures/creature-fixtures.json` (`node test/creature-fixtures.js` rewrites it after a change to `creature.js`). Run `flutter analyze` and `flutter test` in `mobile/`; `flutter build ios --simulator --debug` builds it for the simulator. Its three keys (`lib/keys`, Swift in `ios/Runner/SecureKeys.swift`): the signing key lives in the Secure Enclave and needs Face ID on every use, which the simulator cannot do, so run the simulator with `--dart-define=BOBIX_SOFTWARE_KEY=true` (a plain Keychain key, debug builds only). The key store's own tests: `cd mobile/ios && xcodebuild test -workspace Runner.xcworkspace -scheme Runner -configuration Debug -destination 'platform=iOS Simulator,name=<simulator>' -only-testing:RunnerTests/SecureKeysTests`; through the channel: `flutter test integration_test -d <simulator> --dart-define=BOBIX_SOFTWARE_KEY=true`. `auth` and `box` live in the keychain access group `$(AppIdentifierPrefix)dev.bobix.phone.keys` (`Runner.entitlements`, named in Info.plist as `BobixKeychainGroup`) that the app's extensions list too; erasing a key reaches every group the app sees, so an item an earlier build wrote elsewhere goes as well. The real Face ID check is by hand on an iPhone. To run on a real iPhone, copy `mobile/ios/Flutter/Local.xcconfig.example` to `Local.xcconfig` (git ignores it) and put your own Apple team id there; the project file holds no team. Flavors: `--dart-define=BOBIX_FLAVOR=dev` lets the app use plain `ws://` to a relay on localhost (`relay --dev`); `prod`, the default and what any other value means, refuses every plain connection (`lib/config/flavor.dart`). The Rust core (`mobile/rust`, a thin flutter_rust_bridge layer over `crates/bobix-link`; the generated glue is `lib/src/rust` and `mobile/rust/src/frb_generated.rs`): after changing `mobile/rust/src/api`, run `flutter_rust_bridge_codegen generate` in `mobile/` (version 2.13.0); `cargo build --manifest-path rust/Cargo.toml` then `flutter test` checks the crate's `vectors.json` through the bridge (`mobile/integration_test/support/crate_vectors.g.dart` is its copy for the simulator test (`node mobile/tool/crate-vectors.mjs` rewrites it) and must be kept equal); `flutter test integration_test/core_vectors_test.dart -d <simulator>` runs the same checks in the app. The phone's link (`lib/link`): `pairing_service.dart` pairs from a scanned code and stores nothing before the computer's `paired` arrives; `computer_link.dart` keeps the one connection to the relay (foreground only, backoff 1 s to 60 s, an `ack` for every packet, a `sync` on connect, `relayMoved`, the relay's `revoked`, a sealed `revoked` that erases everything); `link_phone_data.dart` turns the computer's `home` and `detail` into what the screens show. The last `home` is kept in `link-state.json` in Application Support (not backed up); a `detail` is never written. `test/link_integration_test.dart` runs the real `relay --dev` and `fake-desktop --scenario`: build them first (`cargo build --manifest-path relay/Cargo.toml`, `cargo build --manifest-path crates/bobix-link/Cargo.toml --example fake-desktop`) and the Rust library, then `flutter test`. A simulator has no camera: in the `dev` flavor the scan screen also takes the code typed in; start `fake-desktop --relay ws://127.0.0.1:8787` next to `relay --dev` and paste the code it prints. `.github/workflows/mobile.yml` runs all of it on a pull request that touches `mobile/`, the crate, the relay, the strings or the pets |
| `src/ui/gif/` | GIF code copied unchanged from gifenc and gifuct-js (MIT, licenses next to it) |
| `src/hook/hook.js` | The forwarder Claude Code runs on each hook event (copied to `~/.claude/bobix/`) |
| `test/` | `core.test.js`: Node tests (`node --test`), run by `npm test`. `overlay.test.js` and `layout.test.js`: the pets overlay, and the panel with the pets card (no layout shift), in headless Chrome (CI runs them; skipped without Chrome); `chrome-helper.js` starts Chrome for both, with limits and cleanup. `fixtures/`: their input files |
| `docs/` | This documentation; `docs/media/` holds the screenshots and `demo.gif`, `docs/releases/` the release notes |
| `site/` | The project's web page (published by `.github/workflows/pages.yml`) |
| `crates/bobix-link/` | The link crate: the wire format and the crypto that the desktop, the phone-access relay and the phone app will share (see [The link crate](#the-link-crate)). Not used by `src-tauri` yet |
| `relay/` | The relay for phone access: forwards sealed packets between a desktop and its phones, holds one for an offline device for at most 10 minutes, tells each side who is online, sends pushes. Holds no key that opens a packet (see [The relay](#the-relay)). Not used by `src-tauri` yet |
| `.github/` | Workflows (`smoke.yml`, `release.yml`, `claude-review.yml`, `pages.yml`, `link.yml`, `relay.yml`), `dependabot.yml`, the pull request template |
| `CLAUDE.md`, `.claude/settings.json` | The working rules and shared permissions for Claude Code sessions in this repository |

## Build installers

```bash
npm run tauri build   # installers land in src-tauri's build folder, under bundle/
```

Installers have fixed names (`Bobix-mac-arm64.dmg`, `Bobix-mac-x64.dmg`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb`, `bobix-linux-arm64.deb`), given by [the release workflow](../.github/workflows/release.yml), so `releases/latest/download/<name>` always serves the newest version. Keep the names there and the links in the READMEs and `docs/install.md` in step. Every push to `develop` builds them too: they are that run's artifacts on GitHub Actions, to try a build before it is released. Pull requests only run the smoke workflow; to try a PR's installers, start the release workflow by hand on its branch (Actions → release → Run workflow).

## Update feed

Bobix updates itself from signed update files ([update.rs](../src-tauri/src/update.rs), Tauri's updater plugin). The release workflow builds them next to the installers and a tag build publishes two update lists (a small JSON file naming the newest version, and for each system its file and signature):

| List | Where | Who reads it |
|---|---|---|
| `latest.json` | attached to each stable release: `releases/latest/download/latest.json` | everyone |
| `test.json` | attached to the pre-release `test-channel` (not a version: do not delete it, and never make it "latest"), replaced by every tag, stable or `-rc`: the newest build of any kind | only Settings → **Get test builds** on |

Both URLs are `plugins.updater.endpoints` in `tauri.conf.json` (stable first, test second). The update files are named like the installers (`Bobix-mac-arm64.app.tar.gz`, `Bobix-windows-x64.exe`, `bobix-linux-amd64.deb` and their `.sig`); [`update-manifest.mjs`](../.github/scripts/update-manifest.mjs) writes the lists and a tag build stops, before it publishes anything, when a file or a signature is missing. The app installs only a version greater than the running one, so a test build is never a step back from a release, and a release is a step up from its own test builds.

**Signing key (the owner's, never in the repository, a log or a session).** The key pair exists (key id `6C8CB60511EB2B2F`): its public key is in `plugins.updater.pubkey` of `tauri.conf.json`, and the private key and its password are secrets of the GitHub Environment `release`. The next step is a test tag (`v0.2.0-rc.N`) to prove a real signed update: install the previous test build by hand, tag the next one, turn on Get test builds, and update through the app. To make a new pair (a rotation, below):

1. On your own computer: `npm run tauri signer generate -- -w ~/bobix-updater.key`. It makes the private key (that file, protected by the password you type) and a public key (`bobix-updater.key.pub`).
2. Store them as secrets of the GitHub Environment `release` (Settings → Environments; only tags matching `v*` may use it), not as repository secrets: `gh secret set TAURI_SIGNING_PRIVATE_KEY --env release < ~/bobix-updater.key` (the file is read from your computer, nothing is printed) and `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --env release` (it asks for the password). Keep one backup of the key file and the password somewhere only you can open (a password manager).
3. Put the public key's contents (it is not secret) in `plugins.updater.pubkey` of `tauri.conf.json`, in a pull request. A build whose `pubkey` is not a public key (a placeholder) announces new versions with the link only: it cannot install them.

The key reaches only runs for a `v*` tag: a develop push or a run by hand builds the installers as before, with no update files. (The secrets live in the Environment `release`, which only `v*` tags may use, and the build job joins it only on a tag run. A tag runs the workflow of the commit it points at and tags are not protected, so the environment's tag rule is what keeps a branch from reaching the key.) A tag build stops before it publishes when the secrets are missing, `plugins.updater.pubkey` is still the placeholder, the tag is not `package.json`'s version, or an update file was signed with a key other than that public key's. **Rotating the key:** an installed Bobix checks signatures with the public key it was built with. Release one version that carries the **new** public key, signed with the **old** private key; once people are on it, sign with the new key. **If the private key is lost** with no such release made, no installed copy can update in-app again: publish a build with the new key, and tell people in the release notes to install it by hand (links in the [install guide](install.md)). The same hand install is needed once for anyone on a version from before the updater.

**Try an update on your own computer** (nothing of yours is touched): make a throwaway key (`tauri signer generate -w <scratch>/k.key`), build two versions with `tauri build --debug --config <file>` where the file sets `version`, `bundle.createUpdaterArtifacts`, `plugins.updater` (`pubkey` of the throwaway key, `endpoints` of `http://127.0.0.1:<port>/latest.json` and `/test.json`, `dangerousInsecureTransportProtocol: true`), sign with `TAURI_SIGNING_PRIVATE_KEY`, write the lists with `update-manifest.mjs --partial --base http://127.0.0.1:<port>`, serve the folder and open the older build with a scratch `BOBIX_HOME`, `BOBIX_DATA_DIR` and `BOBIX_PORT`. Delete the key afterwards.

## Branches

- **`main`** is the latest stable release and the default branch. It does not take direct changes: it moves to a new version only when that version is released as stable.
- **`develop`** is where work lands. Open pull requests against `develop`; every push to it builds the installers as artifacts, and pre-releases (`v0.2.0-rc.N`) are tagged from it.
- **`v1`** carries maintenance of the v1 line (Electron, v0.1.x) from the commit `main` had before 0.2.0. A fix for v1 is a pull request to `v1`, shipped as a `v0.1.x` tag on `v1`. Fix the same bug on `develop` too if the code still exists there. **The v1 line ends with 0.2.0:** once 0.2.0 is released, `v1` gets no more fixes.

`develop`, `main` and `v1` are protected: changes come in by pull request only (also for administrators), with no force push and no deletion. On `develop` and `main` the three smoke checks (`tauri (macos-latest)`, `tauri (windows-latest)`, `tauri (ubuntu-22.04)`) must pass before a pull request can merge; the smoke workflow runs on every pull request, whatever files it changes, so a docs-only pull request can merge too. It also runs on pushes and on `v*` tags, and the release workflow on `v*` tags, so both work on any of the three branches. Tags are not protected. Repository settings (branch protection, Dependabot alerts, private vulnerability reporting, the review secret) are the owner's: a session does not change them.

## Release

1. Bump `version` in `package.json` (the app reads it from there) on a branch such as `chore/release-0.2.0` cut from the branch you release from (`develop`, or `v1` for a v0.1.x fix), and open a pull request to that branch. Those branches are protected, so the bump cannot be pushed straight to them: the smoke checks run on the pull request and the owner merges it.
2. When it has merged, tag the merge commit: `git fetch origin && git tag v0.2.0 origin/develop && git push origin v0.2.0` (`origin/v1` for a v0.1.x fix). Tags are not protected. The release workflow builds on macOS, Windows and Ubuntu and attaches the installers to a GitHub Release. A tag with a dash (`v0.2.0-rc.1`) makes a pre-release, which does not become "latest".
3. Add release notes to the release. For a `v0.1.x` fix released from `v1` (the tag runs `v1`'s workflow): it must not become GitHub's "latest", or `releases/latest/download/latest.json` (the stable update list of the 0.2 line) stops existing and 0.2 users stop seeing new stable versions. Give `v1`'s release workflow `make_latest: false` for `v0.1.*` tags, as this branch's has, or run `gh release edit v0.1.x --latest=false` right after. The first release with the updater says so: copies before it (0.1.x and earlier test builds) update by hand once. Say too that **Get test builds** is off by default, so a tester on a test build sees the next one only after turning it on in Settings → App.
4. For a release with changes people should know about, rewrite the tour's steps (`STEPS` in `src/ui/tour.js`, each pointing at a real element, with its `tour.<key>.title` / `.body` strings in EN and VI), mark the one about this release `news: true`, and set `NEWS` in `src/ui/i18n.js` before tagging: the panel shows everyone updating to that version that one step once, as a single bubble (Done marks every step seen), never a new install; Setup → "Show the tour again" shows the whole tour. What a group or a setting does goes on its "?" instead (`HELP` in `src/ui/tour.js`, strings `help.grp.<key>` / `help.row.<key>`).

## CI

[The smoke workflow](../.github/workflows/smoke.yml) runs on every push: node and Rust tests, then the app on each OS with a pretend and a real session (`BOBIX_SMOKE`, see `src-tauri/src/smoke.rs`): pets on screen, a held permission request answered, the panel and its bridge, a pet picture imported, the Skills screen's audit actions on a scratch skill and the session's `CLAUDE.md` (each undone), the tray, start at login, a session opened from Overview, ⌘K, the pet card and the tray (and an ended one). On Linux it takes the Wayland path (XWayland) and drags a pet with the real mouse (`xdotool`), then points at a pet: the line of pets must stop. To run the same check on your machine before pushing (scratch folders, so it never touches your own settings or hooks):

```bash
cd src-tauri && cargo build && cd .. && T=$(mktemp -d) && BOBIX_SMOKE=$T/out BOBIX_PORT=4767 BOBIX_DATA_DIR=$T/data BOBIX_HOME=$T/home ./src-tauri/target/debug/bobix; cat $T/out/report.json
```

It exits by itself and `report.json` must say `"pass": true` with empty `pageErrors`. Started from inside a Claude Code session, unset the `CLAUDE*` variables first (`CLAUDE_PID`, `CLAUDE_CODE_HOST_SESSION_ID`): the pretend session would otherwise belong to that session and wait for its parent link, and the check counts one pet too few. The pages load only the files `src-tauri/src/server.rs` lists in `ASSETS` (a new file under `src/ui/` must be added there; `npm test` checks it).

[The release workflow](../.github/workflows/release.yml) builds the installers, installs each (over the last Electron-based release on Windows and Linux) and runs the same check on the installed app.

## The link crate

`crates/bobix-link` holds the protocol and the crypto for phone access in one place: keys, the sealed packet, counters, messages, signed commands, the pairing code and the frames spoken to the relay. Every primitive comes from an audited RustCrypto crate; the crate decides only how they combine. It is a path dependency, not part of a Cargo workspace (`src-tauri` keeps its own `target`), and it does no file or network I/O, so the same code can sit behind the phone app's bindings. Nothing in `src-tauri` calls it yet.

```bash
cd crates/bobix-link && cargo test
```

- `tests/vectors.json` is a committed file with fixed keys, nonces and expected bytes. The tests compare against it and never rewrite it. After a deliberate wire change, run `cargo test --test vectors -- --ignored print_vectors`, review the diff and commit it. Signature vectors are verify-only (a phone's secure hardware does not sign the same bytes twice).
- `fuzz/` holds two cargo-fuzz targets (`open_envelope`, `parse_relay_frame`); they need a nightly toolchain: `cargo install cargo-fuzz && cd crates/bobix-link && cargo +nightly fuzz run open_envelope`. The `link` workflow runs each for 45 seconds on every pull request that touches the crate.
- Wire details a port must follow: `SessionInfo.canStop` says Stop is possible (only for a session waiting on a held request; absent means false); a plan note and each answer to a question are at most 2000 bytes; a refused `start` carries the reason `folder` (the repository's folder is missing) or `disk` (not enough free space); `error { grant }` carries `peer`, the id of the phone whose push grant ended.
- The fake tools are examples and use made-up data only; they speak plain `ws://`, so use them with the in-process stub or with a relay started with `--dev` on localhost:
  ```bash
  cd crates/bobix-link
  cargo run --example relay-stub                      # prints ws://127.0.0.1:PORT
  cargo run --example fake-desktop -- --relay ws://127.0.0.1:PORT --scenario normal   # prints a QR string
  cargo run --example fake-phone -- --relay ws://127.0.0.1:PORT --qr 'bobix://pair?...'
  ```
  `fake-desktop --scenario list` names the 20 scenarios; `tests/fakes.rs` runs each of them in-process. `fake-phone` reads commands from standard input (the list is at the top of its source file) and signs with a software key.

[The link workflow](../.github/workflows/link.yml) runs on every pull request, but its jobs skip themselves when nothing under `crates/bobix-link/` changed (a skipped job counts as passing for a required check). Jobs: `link` on macOS, Windows and Ubuntu (clippy, tests, the fake tools built and kept as an artifact), `fuzz`, `audit` (known vulnerabilities in the dependencies, with cargo-deny), and `link-ok`, which fails when any of them failed. They have their own cache keys, so they never evict the app's cache. The owner makes `link-ok` a required check of `develop`, `main` and the phone-access integration branch `feat/phone-access` after the workflow has run once; it is the only name to require, because a skipped matrix job does not report one check per system.

## The relay

`relay/` is an axum server (a path dependency on `bobix-link`, its own `Cargo.lock`, not part of a workspace). It authenticates a device with a challenge signed by its `auth` key, keeps one in-memory mailbox per recipient (ttl at most 600 s, 64 packets, 2 MiB) until the recipient acknowledges, delivers only between devices that allowed each other, forwards up to four `pairSend`s per open pairing token, shows presence (`online`, `offline` after 30 s without a pong, `asleep`), keeps revocations in a small file per desktop for 90 days (a revocation cuts only the path between that desktop and that device; the device is told with `error { revoked }` and keeps its other desktops), and with `INVITE_FILE` set serves only invited computers. Pushes go through a `PushSender` (APNs, FCM, another relay's gateway, or a recording fake); `POST /v1/push` is the gateway for relays without credentials. It never parses a `body` and has no code or dependency that could open one. Only a relay with its own APNs or FCM credentials (or `--dev`) serves the gateway route.

```bash
cd relay && cargo test
cargo run -- --dev          # plain ws on 127.0.0.1:8787, recording push fake, GET /healthz, GET /dev/pushes
```

- Configuration is by environment variables only; without `--dev`, `RELAY_PUBLIC_URL` (the relay's own address as devices know it, such as `wss://relay.example`; a device signs it when it logs in, so a signature made for another relay is refused with `relayUrl`, and a wrong value shows in the log as "signed for … but this relay's RELAY_PUBLIC_URL is …"), `RELAY_ID` (the id push grants name) and `REVOKED_DIR` are required and the relay says which one is missing. With `--dev` the public URL is `ws://` plus the bind address unless `RELAY_PUBLIC_URL` is set. Others: `RELAY_ADDR` (default `0.0.0.0:8787`; `127.0.0.1:8787` with `--dev`), `INVITE_FILE` (one device id per line, `#` comments), `GATEWAY_KEYS_FILE` (one gateway key per line, optionally followed by the relay id it belongs to), `LOG_DIR` (one file per day, 14 days kept), `APNS_KEY_FILE` with `APNS_KEY_ID`, `APNS_TEAM_ID`, `APNS_TOPIC`, `FCM_SERVICE_ACCOUNT_FILE`, and `PUSH_GATEWAY_URL` with `PUSH_GATEWAY_KEY_FILE`. Credentials are read from the files named, never from the repository.
- `--dev` refuses to listen on anything but loopback (it keeps no revoked list on disk and sends no real push); `RELAY_DEV_FORCE_BIND=1` overrides this on purpose and the relay says so at start.
- Limits: a socket has a queue of 256 outbound frames and is closed when it is full (a reader that does not read); a device may send 100 frames a second; a client address may hold 8 sockets that have not authenticated and open 120 connections a minute; 1000 mailboxes and 256 MiB at most are held in all; the allow-list and push tokens of a device that left are dropped after an hour; pushes through the relay's own credentials are limited per token and per sending device. The client address is the connection's, or, behind a proxy you trust, the last value of the header `CLIENT_IP_HEADER` names (for example `X-Forwarded-For`); set it only when the proxy overwrites that header, otherwise a client could pick its own address. `INVITE_FILE` is looked at again every 5 seconds, also for desktops that are connected.
- A desktop can revoke only a device that confirmed a pairing with it, meaning it sent the desktop a packet the desktop had allowed; that record is kept next to the revoked list (`<desktop id>.paired` in `REVOKED_DIR`, 90 days), so a revoke works after a relay restart. A desktop that merely received a `hello` cannot affect the phone. A fresh pairing the desktop opens lifts an old cut.
- `--dev` also serves the socket on `/`, because the fake tools of the link crate connect to exactly the address they are given: `fake-desktop --relay ws://127.0.0.1:8787`. A real client connects to `/v1/ws`.
- Logs hold device ids and counts, never a body or a token (`tests/push.rs` checks this).
- Deploy: [`relay/README.md`](../relay/README.md) covers what the relay can see, self-hosting, every setting and the Railway steps; `relay/Dockerfile` is built from the repository root (`docker build -f relay/Dockerfile .`). Not done yet: the deploy itself.

[The relay workflow](../.github/workflows/relay.yml) mirrors the link workflow: it skips its jobs when nothing under `relay/` or `crates/bobix-link/` changed; jobs `relay` (clippy and tests on macOS and Ubuntu), `audit` (cargo-deny) and `relay-ok`, the one check to require.

## Demo and screenshots

The screenshots and `demo.gif` in `docs/media/` were taken from a debug build in a scratch world (its own port, data folder and `BOBIX_HOME`, made-up repositories and transcripts), capturing only Bobix's own windows. Keep them few and small (about 0.7 MB in all), and take new ones when the pages change. The app icon comes from one 1024 px picture: `npx tauri icon <png>` writes `src-tauri/icons/`; the tray icon is drawn in `src-tauri/src/tray.rs`.
