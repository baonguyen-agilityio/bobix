# Bobix

A Tauri desktop app: Rust core in `src-tauri/src`, plain JS pages in `src/ui`. Work lands on `develop`; `main` is the
stable branch. How to run it and how it is built: `docs/development.md`. What is open or unverified: `docs/roadmap.md`.

## Test

- `npm test` runs `test/core.test.js` only. `test/overlay.test.js` needs Chrome; run it by hand when you touch the pets overlay.
- `cd src-tauri && cargo test` for the Rust side.
- The smoke run starts the real app (command in `docs/development.md`, "CI"). It needs `BOBIX_SMOKE`, a free `BOBIX_PORT`,
  and a scratch `BOBIX_DATA_DIR` and `BOBIX_HOME`; unset the `CLAUDE*` variables first when started from a Claude session.

## Working rules

- One git worktree per task, cut from `origin/develop`: `git fetch origin && git worktree add .claude/worktrees/<n>-<slug> -b <type>/<n>-<slug> origin/develop`.
  Do not edit the main checkout. Every task has a GitHub issue first.
- Pull requests target `develop` and say "Closes #N". Conventional commits (`feat(scope): ...`), no AI mention.
- Never merge, and never `gh pr merge --auto`: the owner approves every merge. Answer every review comment (fixed, or not an issue with the reason) before asking.
- Every string the app shows exists in EN and VI (`src/ui/i18n.js`; the Rust side reads the same file).
- A new Tauri command goes in all three lists: `generate_handler!` and `COMMANDS` in `src-tauri/src/main.rs`, and `src-tauri/build.rs`. A Rust test checks them; the pages are refused at run time otherwise.
- A new file under `src/ui/` goes in `ASSETS` in `src-tauri/src/server.rs`; `npm test` checks it.
- Verify a change in the real built app (real IPC), not only in tests with a stubbed bridge: a throwaway `BOBIX_HOME`,
  `BOBIX_DATA_DIR` and `BOBIX_PORT`, so it never touches your own settings or hooks.
- Screenshots, GIFs and demos use made-up data only: no real skills, sessions, repositories or paths. Keep them out of the repository unless the docs need them.
- A new screen, a large layout change or a new area starts as a prototype outside the repository, approved by the owner before the issue is written.
- Update `docs/roadmap.md` when an item lands, ships or turns out unverified.
- Every pull request also gets an automatic docs check (`.github/workflows/claude-review.yml`) that comments where a README, doc, roadmap line or app string no longer matches the change. Answer its comments like the code review's. That workflow file must be identical on `main` and `develop`, or the action skips and the job warns.
- Delete what a task leaves behind when it ends: test copies, scratch folders, build output, merged worktrees.
- The repository is public: no secrets, tokens, `.env` files, local paths or personal details in code, docs, issues or pull requests.

## Which model for which step

- Read-only exploration, running checks and watching CI: a Haiku subagent at low effort.
- Writing code, config, docs and prototypes, merging, fixing review findings: Sonnet at medium.
- A risky feature (update, signing, anything security): Sonnet at high.
- A hard diagnosis, a design fork, or the independent review before a merge (by a session that did not write the code): Opus at high; xhigh only for a security design.
- The automatic review and docs check (`claude-review.yml`): Opus at high.
- The coordinating session stays thin and hands work out. One session per task; start a new one when the task changes.
