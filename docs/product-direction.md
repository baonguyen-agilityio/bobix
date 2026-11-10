# Product direction (v3)

Agreed with the owner on 2026-11-10 and tracked in [#110](https://github.com/baonguyen-agilityio/bobix/issues/110). This is a record of decisions, not a promise of dates and not a spec. Nothing here is built yet. The current promises in [Privacy](privacy.md) stay true until the code for each step ships and that step updates them.

## What Bobix becomes

Today Bobix shows each Claude Code session as a pixel pet, with a panel for the Board, approvals, habits and notifications. It grows from "a pet per session" into a personal assistant for working well with Claude Code: see what a session costs, keep the setup lean, run a ticket from pick to result, turn habits into skills, and settle a team discussion with a record.

**Bobix stays a personal tool**: one developer, their own machine, a personal Claude subscription (Pro or Max). It is not a team or company product. No manager views, no per-person tracking.

## Build order

| Step | Area | Issue |
|---|---|---|
| 1 | Measure usage | [#84](https://github.com/baonguyen-agilityio/bobix/issues/84) |
| 2 | Skills audit, splitting sessions, choosing models | [#111](https://github.com/baonguyen-agilityio/bobix/issues/111) |
| 3 | Ticket flow: pick, plan, run, log (GitLab first) | [#112](https://github.com/baonguyen-agilityio/bobix/issues/112) |
| 4 | Habits become personal skills | [#113](https://github.com/baonguyen-agilityio/bobix/issues/113) |
| 5 + 6 | War room over Tailscale, and its security | [#114](https://github.com/baonguyen-agilityio/bobix/issues/114) |

Steps 2 to 4 rely on step 1, because it adds the opt-in to read session content and the numbers that tune their rules.

## Where today's promises change

These are the places where this direction differs from what [README](../README.md), [Privacy](privacy.md) and the [roadmap](roadmap.md) say today. Each one changes only when its step ships, with the user's consent.

| Today | Changes to | Step |
|---|---|---|
| Bobix reads only the tail and head of a transcript for the title, the last answer and token usage. Habits never store what you write. | Reading session content (prompts, answers) becomes an **opt-in**, used to derive facts. Only derived facts are stored (task type, counts), never raw text. | 1, 4 |
| Bobix never edits Claude's settings or skill folders (it only adds and removes its own hooks). | Bobix **may edit Claude skills and a project's `CLAUDE.md`**, only when the user clicks, always with a backup and an undo. | 2, 4 |
| Habit suggestions are simple local rules, "without any AI". | Bobix may ask **Haiku through the user's own `claude -p`** on this machine (task type, summaries, weekly habit scan). The content then goes to Anthropic through the user's own Claude, as any Claude Code use does. | 2, 3, 4 |
| Bobix opens a session with a prompt you edit; it starts nothing by itself. | After you approve a plan, the session **runs to the end in its own worktree** with Claude's own auto mode. | 3 |
| Bobix accepts connections on `127.0.0.1` only. | The war room adds a **second port bound to the tailnet interface only**. The panel stays on `127.0.0.1` with its token. | 5, 6 |

Stays as it is: Bobix sets no standing rules (use Claude's own auto mode), reads and stores no Claude login credentials and no GitLab or GitHub token, and sends nothing on its own.

## Numbers behind the decisions

Measured on the owner's Mac. Cost figures are **API-equivalent estimates** (token counts times public API prices). They are not what a subscription is billed, and they will differ on other machines.

- **Usage:** transcripts of the last 14 hours of active sessions. About 99% of turns were Opus. About 75 to 80% of the estimated cost was cache reads (long sessions re-reading their context every turn); output was about 8%. The main "lead" session cost about $1,300; sub-sessions cost $30 to $145 each.
- **Skills:** about 300 skills installed (110 user, 84 project, 108 plugin), about 19k tokens of descriptions in every turn, with duplicate sets (`ck:*` and `ak:*`). The listing itself is only about 2 to 4% of cost.

---

## 1. Measure usage (cost per task)

**Problem.** People cannot see what a session or a ticket costs. The measured data says the biggest lever is session length, then model choice, and nobody sees either.

**Decisions**
- **Unit:** the per-session view comes first. Per-ticket totals come later, as the sum of the sessions linked to an issue.
- **On a subscription, cost means plan limits** (the 5-hour and the weekly limit), not dollars. Show usage against the user's own normal ("3× your usual session"). Dollars may appear only as a labelled API-equivalent estimate.
- **Never read Claude login credentials**, and never call undocumented account APIs to get the quota. That is a Terms of Service risk, and the token unlocks the whole account. `/usage` in Claude stays the source of the real quota.
- **Reading session content locally is allowed, as an opt-in.** Store derived facts only (task type, counts), never raw text, because transcripts can hold secrets.
- **Live warnings:**
  - the session is too long: suggest `/compact` or a new session;
  - a heavy model is doing light work: suggest Sonnet;
  - read-only exploration: suggest a Haiku subagent.
- **The lead session stays thin:** it hands out work, collects results and compacts often.
- **Prices live in an editable table**, not in code, with the date they were set.
- This step is [#84](https://github.com/baonguyen-agilityio/bobix/issues/84). Its existing points stay: only data Claude already writes locally, an estimate marked as such, hidden when the data is missing, and a switch to hide cost.

**Out of scope**
- Reading credentials or calling account or quota APIs.
- Per-ticket totals in the first version.
- Storing raw prompt or answer text.
- Any per-person or team view.

**Open questions**
- The exact thresholds for "too long".
- Which local files carry usage per turn for the CLI and for the desktop app on macOS, Windows and Linux (verified on macOS transcripts only).

## 2. Skills audit, splitting sessions, choosing models

**Problem.** Users install hundreds of skills, often in duplicate sets, and do not know how to split work across sessions or which model to use. The skill listing itself is cheap. The real harm is the model picking the wrong skill, and skill workflows making sessions long.

**Decisions**
- **Skills audit, per project:** list the installed skills, flag those unused for 30 days (from transcripts) and duplicates, and suggest removing or disabling them.
- **Bobix may apply the change when the user clicks.** This reverses the "never write Claude settings" rule for this case only. It always keeps a backup and offers an undo.
- **Model routing**, so users don't have to learn it:
  - **(a) Base.** After the user agrees, Bobix adds a short "when to split, which model" rule block to the project's `CLAUDE.md`.
  - **(b) At Start on a ticket.** Bobix asks Haiku (`claude -p` on this machine) to classify the task and proposes a split, for example explore → Haiku subagent, edit → Sonnet, review → Opus, or "small: one Sonnet session". One click opens the sessions with those models. The CLI can set a model (`claude --model`); the desktop app is an open question.
  - **(c) Teach in place.** When a session uses an unsuitable model, the pet gives a one-line tip with the reason.
- Start with a few simple rules and tune them with the step 1 data.

**Out of scope**
- Promising "optimal AI splitting".
- Changing skills, settings or `CLAUDE.md` without a click, a backup and an undo.
- Writing Claude settings for any other purpose.

**Open questions**
- Can a model be preset when opening a new Claude desktop session? Not through its documented link (`claude://code/new` takes only `q`, `prompt`, `folder` and `file`), checked for [#112](https://github.com/baonguyen-agilityio/bobix/issues/112): the ticket flow says which model to pick there and offers the terminal command, which presets it.

## 3. Ticket flow: pick, plan, run, log

**Problem.** Starting a ticket is manual, and afterwards nobody can say what was assumed along the way or why the result looks as it does.

**Decisions**
- **GitLab first**, through the user's own `glab`. Jira and Trello come later; for those, either keep the token in the OS keychain or use the vendor's own connector.
- **One approval gate: approving the plan**, which must list its assumptions.
- **After approval** the session runs to the end in its own worktree, using Claude's own auto mode. Bobix sets no standing rules.
- **It always stops and asks only for:** pushing to the main branch, deleting data, calling unknown hosts, and touching secrets. Nothing else is on the list for now.
- **Work log, for the user only, never shared:** the plan, the steps, the assumptions made along the way (highlighted), and the result or what blocks it.
  - The session writes it from a template that Bobix puts in the prompt.
  - Bobix fills gaps with a Haiku summary (`claude -p`).
  - The pet shows a mark when a new assumption appears; the session does not stop.
- **Disk:** each worktree can take about 5 GB with a build. Limit parallel worktrees to the free disk, and offer to clean up finished ones.

**Out of scope**
- Jira and Trello in the first version.
- More stop conditions, or any standing rules set by Bobix.
- Sharing the work log with anyone.

**Open questions**
- Jira and Trello credential storage: OS keychain, or the vendor's own connector.

## 4. Habits become personal skills

**Problem.** The same requests come back every week, but each time they are typed from scratch. Today's habit log has no prompt content, so it cannot tell what a repeated request is.

**Decisions**
- **"Repeated"** means the same kind of request at least once a week, two weeks running.
- **Bobix scans once a week** with Haiku over prompt content. This needs the opt-in from step 1.
- **Drafting:** reuse Claude's skill-creator with the good sessions as examples. The user approves the draft.
- **Install to the user's own `~/.claude/skills`**, not the project, because habits are personal.
- **Before creating, check for a similar skill** and improve that one instead of adding another. This ties to the step 2 audit.
- **Improve only on evidence** (cost, rework, the user's edits per use). Each change is a version with a rollback.
- **Auto-made skills unused for a month** are suggested for removal.

**Out of scope**
- Installing into a project, or creating or changing a skill without the user's approval.
- Changes made without evidence.
- Scanning without the opt-in.

**Open questions**
- Built in [#113](https://github.com/baonguyen-agilityio/bobix/issues/113). The weekly scan has a switch of its own besides the step 1 opt-in, because it sends prompts to Haiku rather than only counting them.
- "Similar skill" is a word-overlap test (the skills audit's), not a judgement by a model; it may miss a skill worded differently.
- The thresholds for "improve on evidence" (3 uses, then 3 follow-up prompts or 1 fix request per use) are first guesses.

## 5. War room over Tailscale

**Problem.** Discussions in Slack get lost or misread. Teams need structure and a final record, not another chat app.

**Decisions**
- **Network:** the company tailnet.
- **Purpose:** structure and a final decision record. One room per problem, with the frame problem → options → pros and cons → decision.
- **Hosting:** the room creator's machine hosts it and the others connect. Real peer-to-peer sync is out of scope.
- **AI:** exactly one summarizer, on the host's machine. People contribute by hand. This keeps company content from flowing into several personal Claude accounts.
- **Output:**
  - By default, a polished, self-contained HTML decision record saved on every participant's machine. It stays inside the tailnet.
  - Optionally, a manual "Publish as Claude artifact" button that warns it goes to claude.ai, in the host's personal account.

**Out of scope**
- Another general chat app.
- Peer-to-peer sync.
- More than one AI summarizer per room.
- Remote control of other machines (Tailscale and SSH already do it).

**Open questions**
- Can a headless `claude -p` publish an artifact?

## 6. Security of the war room

**Problem.** Opening a port to other people on the network changes the security model, which today is "this computer only".

**Decisions**
- The panel stays bound to `127.0.0.1` with its token. The room uses a **separate port, bound to the tailnet interface only**.
- **Identify people by Tailscale identity** (`tailscale whois`). The host invites named people.
- **Nothing is sent automatically**: only what someone types or pastes. Never transcripts, logs or session data.
- **Redact secrets before sending**, and keep an **audit log** of who joined and saw what.

**Out of scope**
- Sending session data of any kind.
- Anyone on the tailnet joining without being invited.
- Remote control of other machines.

**Open questions**
- None recorded yet beyond step 5's artifact question.

---

## Open questions in one list

1. Can a model be preset for a new Claude desktop session? (step 2) Not through the documented link; see step 2.
2. Can a headless `claude -p` publish an artifact? (step 5)
3. The exact usage thresholds for "too long". (step 1)
4. Where Jira and Trello credentials are stored. (step 3)

## Related

- [Roadmap](roadmap.md): the existing "Cost per session" and "Several machines" items overlap steps 1 and 5; they fold into these steps when the work starts.
- Companion mod issues [#85](https://github.com/baonguyen-agilityio/bobix/issues/85), [#86](https://github.com/baonguyen-agilityio/bobix/issues/86) and [#88](https://github.com/baonguyen-agilityio/bobix/issues/88) are separate and not changed by this direction.
