# Panel redesign prototype (#116)

A clickable prototype of the new panel, in one self-contained file: [index.html](index.html). No app code changed. Open the file in a browser; it needs no network, fonts or scripts from outside. Every figure in it is sample data, and the page says so.

- Screens: `#/overview`, `#/session/s4` (any session id `s1`…`s7`), `#/skills`, `#/design`. Sessions shows a plain list; Work, Room and Settings are placeholders that name the issue they arrive with.
- `?theme=light|dark` forces a theme; `?cmdk` opens the command bar.
- Keys: `⌘K` or `/` opens the command bar (↑ ↓ ↵ esc), `⌘1`–`⌘6` switch screens, `↵` allows the top approval, `⌘Z` undoes the last skill or CLAUDE.md change.
- Shots: [contact-sheet.png](contact-sheet.png) and [shots/](shots/) show every screen and the command bar, in light and dark, at 1280 px and at 420 px.

## Design choices

- **Calm and dense.** A sidebar with icons, labels and key hints, a sticky top bar with a breadcrumb, and panels with hairline borders instead of shadows. The sidebar, top bar and command bar are translucent and blur what sits behind them, like macOS vibrancy.
- **Dark-first, light equally finished.** Both themes are designed together from the same token names. Auto follows the system.
- **One accent.** The coral that the pets already use for "needs you" (`oklch(66% 0.175 33)`, the same value as `--coral` in `src/ui/theme.css`). It marks approvals, the primary button, the active nav icon and the output share in charts. Green, amber, blue and red appear only as small dots and badges, always with a word next to them.
- **One thread to the pets.** Every session row, approval and command-bar item shows that session's real pet, drawn by the fine-sprite code copied from `src/ui/creature.js` (`fineSprite`, `petColor`), so the panel and the desktop pets match. The pets on screen do not change.
- **Rows.** Pet, title with project · issue · current tool below it, and right-aligned meta: model, length against your usual session, state and time. Figures use tabular numbers.
- **Usage against your own normal, not money and not quota.** "2.1× your usual day", "3.1× your usual session", split into re-reading context, new input and output. Bobix never reads the plan's real limits; the page points to `/usage` in Claude for them (see [product direction](https://github.com/baonguyen-agilityio/bobix/pull/115), step 1).
- **Assumptions stand out.** In the work log an assumption is an amber block with a flag, so it can be checked before the result is trusted. The log ends in a result: done, blocked (with the action that unblocks it), or not finished yet.
- **Warnings carry their fix.** Each warning has one or two buttons (Compact now, Start fresh, Use a Haiku subagent, Switch to Sonnet, Review skills) and can be dismissed with undo.
- **Changes to Claude's files are safe.** Disable and remove keep a backup and show an undo toast. Adding the model rules to `CLAUDE.md` asks first, shows the lines, and can be removed again.
- **Narrow (about 420 px).** The sidebar turns into a bottom tab bar with six labelled icons, row meta moves under the title, the skills table drops to name, flags and actions, and buttons grow to touch size.
- **Original.** System font, icons drawn for this file, no other product's name, logo, icons or screens.

## Tokens

Defined on `:root` (light), on `:root[data-theme="dark"]`, and under `@media (prefers-color-scheme: dark)` for Auto. The `.theme-light` / `.theme-dark` classes let the design-system page show both side by side.

| Group | Tokens |
|---|---|
| Surfaces | `--bg` window · `--surface` panel · `--surface-2` hover · `--surface-3` selected · `--sidebar` translucent |
| Lines | `--line` hairline · `--line-strong` controls |
| Text | `--ink` · `--ink-2` · `--ink-3` meta (≥ 4.5:1 on panels in both themes) |
| Accent | `--accent` · `--accent-ink` text · `--accent-bg` tint · `--accent-solid` + `--on-accent` primary button |
| Status | `--ok` done · `--warn` warning, assumption · `--info` working · `--err` blocked, each with a `-bg` tint |
| Charts | `--c-ctx` re-read context · `--c-in` new input · `--c-out` output (= accent) · `--c-usual` dashed "usual" line |
| Type | system UI font and mono; sizes 11 · 12 · 13 (body) · 15 · 20 · 28 |
| Space | 4 · 8 · 12 · 16 · 20 · 24 · 32; radius 5 badge · 8 control and row · 12 panel · 14 dialog; rows 36 px, 44 px narrow |

## Old tabs → new screens

| Today's panel | New place |
|---|---|
| Sessions: the board's Needs you, Working, Done, Idle groups | Overview (needs you, running now) and Sessions (all, grouped by state) |
| Sessions: the detail pane (log, answer, report) | Session detail, with models, usage over time, work log and warnings |
| Sessions: To do (assigned issues) and Start on an issue | Work, when the ticket flow ships (#112) |
| Sessions: habit suggestions | Overview warnings for now; their own place when habits become skills (#113) |
| Skills: your role, pinned skills, the projects' skills | Skills, which adds the audit and the model-routing rules (#111) |
| Settings and Setup (hooks, pets, notifications, displays) | Settings, as one screen with sections |
| Feedback dialog, tray actions | ⌘K command bar actions |
| — | Room, when the war room ships (#114) |

## Open questions

1. Is the accent right as the only colour for both "needs you" and the primary button, or should primary buttons go neutral so coral means only "needs you"?
2. Sidebar on desktop and a bottom tab bar when narrow: does the panel window ever get narrow enough in practice to need it?
3. Should Overview estimate the share of a 5-hour window from your own history, clearly labelled, or stay with "× your usual" only? (The product direction rules out reading the real quota.)
4. "Your usual" is the median of the last 14 workdays in the prototype. Is that the right baseline, and what counts as "too long" (product direction, open question 3)?
5. The desktop pets keep their pastel look; is one shared coral enough of a link, or should the panel also tint each row by the pet's colour?
6. Which language comes first: the app has Vietnamese and English today; the prototype is English only.
