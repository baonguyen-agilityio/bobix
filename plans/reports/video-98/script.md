# #98 A day of work with Bobix: script

151 s, 1920×1080, 30 fps, no sound, English captions. The app was set to English while filming. Made with
**ak:hyperframes** (`hyperframes@0.7.99`), like #96: the video is real captures with captions on top, plain HTML/CSS.
`source/gen.py` writes `source/index.html` from the scene list; `source/index.tpl` holds the styles.

## Story

| # | Time | Picture (all real, see Sources) | Caption |
|---|------|----------------------------------|---------|
| 0 | 0–6 s | Title; the pets of the morning walking | Filmed in Bobix 0.2.0-rc.4 with real Claude Code sessions on the bobix repo. |
| **1** | | **See everything** | |
| 1a | 6–15 s | Seven sessions at work, one pet each, `#84 #85 #86 #88 #17` on their tags | Seven Claude Code sessions on the bobix repo. One pet each. → One project, one creature, a colour per session. |
| 1b | 15–23 s | Six close-ups from the same recordings: reading files, running a command, editing code, needs you, done, resting | The costume says what each session is doing right now. |
| 1c | 23–34 s | #85 reads the repo until its context runs low: battery alarm; its card: Context 14% | #85 has read a lot: under 25% context left, the battery alarm goes off. → Point at the pet: its card shows context left: 14%. |
| 1d | 34–41 s | Resting pets with `[BOB-88] … #88`, `#86`, `#84`, `#17` | A session on an issue's branch shows #88, #86, #84, #17 on its pet… → …and Bobix names the session after the issue: [BOB-88]. |
| **2** | | **Control without opening windows** | |
| 2a | 41–51 s | Board › Needs your approval: #17 (`rm README.vi.md`, waited 1 min, "This deletes or overwrites data") above a `curl` request | Two sessions wait for you. The one waiting longest is on top. → A request that deletes or overwrites data is marked. |
| 2b | 51–60 s | The pets window: "Needs you: Bash · Allow / Deny" over the pet; Allow; the card says "Now running a command" | Answer on the pet itself: Allow or Deny. → Allowed: Claude carries on and runs the command. |
| 2c | 60–69 s | Deny on the #17 request in the panel; the session's Terminal: "Error: Denied from Bobix" | Or in the panel: Deny the delete… → …and Claude gets the answer: Denied from Bobix. |
| 2d | 69–74 s | The Terminal brought forward by a click on a pet | Click a pet and its terminal comes forward. |
| 2e | 74–83 s | Show when needed: a pet floats up under a "!" balloon; the corner pill "7 sessions · 1 need you" and its list with Allow / Deny | Show when needed: a pet floats up only while its session needs you. → The corner pill lists every session, with Allow / Deny. |
| 2f | 83–89 s | Settings › Do not disturb (30 min, "On for 30 more min"); the pill "☾ Do not disturb · 30 min left" | Do not disturb: no pets, no notifications. Requests wait on the Board. |
| **3** | | **From issue to PR** (GitHub on, `baonguyen-agilityio/bobix` issues) | |
| 3a | 89–99 s | The Board late in the day: In progress (5 issues with their sessions), To do, Waiting for review (#98, "PR #99 is open"), Done today (#96, #92, #91, …) | The Board follows the day: In progress with its sessions, To do… → …Waiting for review (PR #99 is open) and Done today. |
| 3b | 99–108 s | Start #87 (the suggested Next up): `/ak:cook` "Suggested · pinned for role Dev", the prompt filled in with the issue's title, description and link | To do suggests the issue to start next, and a skill for your role. → Start opens a new Claude session with this prompt, named after the issue. |
| 3c | 108–114 s | #88 "⚠ 2 sessions, one issue", opened: its steps and its two `[BOB-88]` sessions | Two sessions on one issue? Bobix warns you. |
| 3d | 114–123 s | Stuck work (6): #84, #86, #88, #85, #17 "Session done 1 h ago, no PR yet" with Open PR; #98 "PR #99 open 1 h, nobody reviewed it" with Ask for review | Late afternoon: stuck work, each with its next step. → Done but no PR? Open PR. A PR nobody reviewed? Ask for review. |
| **4** | | **End of day** | |
| 4a | 123–131 s | Today's report: [bobix] done today, waiting for review, "Claude waited for you 9 min in all today" (bobix-dev 7 min, bobix-17 2 min) | Today's report, per project, with how long Claude waited for you. |
| 4b | 131–138 s | Stand-up: done yesterday (closed bobix issues), in progress today, stuck | A stand-up to copy: done yesterday, in progress today… → …and what is stuck. Bobix sends nothing. |
| 4c | 138–145 s | Suggestions for you: "A few more days to see your habits"; Settings › What Bobix remembers: what is kept, never kept, and today's log | Habits: after about five working days, suggestions for what to do next. → What Bobix keeps is listed, on this machine only. |
| 5 | 145–151 s | Icon, version, repo link | Everything stays on your machine. No account, nothing sent. |

## Sources

- **App**: the `develop` build at `ed1d76d` (0.2.0-rc.4 + the #96 docs), `cargo build --release` in this worktree, run
  with its own data folder and Bobix home on port 4791 (`capture/world.sh`), never the person's own Bobix (rc.3 on 4747)
  or its settings. For filming only, the build carried the uncommitted `capture/evpatch.py` (panel window 1440×900, and a
  page script for the panel and pets windows read from env vars); it was reverted after the build. The page script
  (`capture/page-poll.js`) runs what `capture/cmd-server.mjs` queues: the clicks and pointing a person would do
  (`capture/pets-helpers.js`, `capture/shoot.sh`).
- **Sessions**: real Claude Code CLI sessions (Haiku 5.5) in Terminal, in throwaway clones of this repo under
  `/Users/Shared/bobix-day` (objects shared with the repo, own refs, origin set to GitHub with push disabled; never
  pushed). Branches name real open issues: `84-token-cost`, `85-companion-mod`, `86-companion-data`, `88-bobix-next`
  and `88-next-ticket` (two sessions on #88), `17-raise-window`, and one on `develop` with no issue. Bobix's hook is
  given with `--settings`, and user settings are left out (`--setting-sources project,local`), so these sessions
  reported to the filmed app only and nothing was written to `~/.claude/settings.json`. Prompts were read-only questions
  about the code, an edit to a notes file in a clone, `curl -sI` to the repo page, and `rm` requests in a clone (one
  allowed in the clone earlier, the filmed one denied).
- **Issues and PRs**: the real `gh`, read-only, through `capture/gh-bobix.js`. Nothing in bobix is assigned to anyone,
  and the account's assigned issues are in other repos, so "issues assigned to you" reads the open issues of
  `baonguyen-agilityio/bobix`, and "your open PRs" keeps only bobix's. Every other call goes to `gh` unchanged; writes
  are refused. Waiting for review shows #98 because this video's own PR #99 is open.
- **Pets**: two ways. The app's own `/pets` page from the running app in headless Chrome with the app's settings applied
  (`capture/rec-pets.mjs`, 1280×260 CSS px at 1.5×, frames back to back), for the walking line, costumes and battery
  alarm. The real pets window (`screencapture -l`, about 7 frames a second, `capture/ov-rec.sh`) for what needs the app
  around it: the Allow / Deny on a pet, the #85 card, show when needed, the pill and do not disturb.
- **Panel**: the panel window captured with `screencapture -l` (`capture/shoot.sh`).
- **Terminal**: the Terminal windows of the sessions, `screencapture -l`.
- Fonts: the app's own Baloo 2 and Be Vietnam Pro (`src/ui/fonts`, SIL OFL).

## Not shown for real

- **Side sessions walking behind their parent.** Bobix links a side session to its parent from the Claude desktop app's
  own session files; only desktop-app sessions have them. Those sessions report to the person's own Bobix (rc.3,
  port 4747) through `~/.claude/settings.json`, and changing that, or filming the Claude desktop app, was out of bounds.
  The CLI sessions filmed here can't be side sessions, so this is left out rather than faked.
- **The macOS notification with Allow / Deny.** The filmed build is unsigned and not the installed app, and its
  notifications were off; as in #96, the pet, the pill and the panel show the same answer.
- **Opening the exact session on click.** With Terminal, Bobix brings the Terminal app forward (`open -a`), not the
  exact window; only the Claude desktop app opens the exact session (README FAQ). The video says "its terminal comes
  forward", which is what happened.
- **Start → the new session named after the issue.** Start opens the Claude desktop app, which was not to be filmed or
  driven, and no run was started on a real issue: the video stops at the filled-in Start box. The naming itself is
  real: Bobix named every filmed session after its branch's issue (`[BOB-84] feat: tokens, …`), seen on the pets.
- **The stuck-work limits** were set to 1 hour in Settings (their lowest) so the hints showed the same day; the defaults
  are 2 h to 24 h. Two hints Bobix also gave were skipped with their Skip button before filming: a branch with
  "193 commits, no PR" for the clones on `develop` (see below).
- **Habits suggestions.** They start after about five logged working days; habits were turned on for filming today, so
  the Board shows "A few more days to see your habits" and the day's log, not suggestions.

## Found while filming

- Stuck work counts a branch's own commits against `origin/HEAD`. In bobix that is `main`, and work goes to `develop`
  (193 commits ahead), so every branch cut from `develop` shows "193 commits, no PR". A base branch per project (or the
  PR's base) would fix it.

## Rebuild

```bash
cd plans/reports/video-98/source
python3 gen.py
DO_NOT_TRACK=1 npx -y hyperframes@0.7.99 render -o ../bobix-day-98.mp4 -f 30 -q high
```

The capture scripts in `capture/` have this machine's paths; they are kept as a record of how each picture was made.
