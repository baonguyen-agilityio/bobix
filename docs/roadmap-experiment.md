# Experimental roadmap: sessions and workers

This is the roadmap of an experimental line on the branch `experiment/session-workers`. The owner decided it on 2026-11-17. [roadmap.md](roadmap.md) stays the record of what is released and unreleased on the main line, and [product-direction.md](product-direction.md) stays the product direction. Nothing here is built yet. This file is a plan, not a status report, and nothing in it has been tested or verified.

The phase order differs from the 15 Nov phase plan: workflow presets and one session per issue come first, and the worktree block of that plan is dropped (see below).

## 1. The model (decided)

1. **One issue = one session**, running in the repository's own folder. Parallel work means the session spawns workers. All workers write to one shared board (the work log). There is no worktree per session any more.
2. **The board lives outside any worktree.** Today the work log is `.bobix-log/work-log.md` inside the flow's worktree. That has to move. Where it moves to is not designed yet.
3. **One exception keeps a worktree at issue level.** A second issue started in the same repository while another issue is writing code gets its own copy. The owner wants two separate pull requests.
4. **Bobix builds its own coordinator**, in the manner of Google Antigravity's [`/swarm`](https://antigravity.google/docs/swarm). The session becomes the coordinator that splits the work. Each worker owns a set of files. There is one shared board. Bobix adds what `/swarm` does not describe:
   - a hook that blocks a worker from editing another worker's files;
   - permission decisions that stay with the person;
   - warnings for disk, usage and worker count;
   - a read-only reviewer.
5. **Merge always needs the owner's approval**, in every workflow preset, Full included.
6. **A session that edits a file when its preset has no implement step:** Bobix asks once ("go on to implement?"). Yes means the session continues and is not asked again.
7. **Workflow presets.** There is one fixed order of 8 steps:
   create issue → plan → follow-up issue → implement → test → review → PR → merge.
   - "Create issue" exists only when starting from an idea. The Start box has a checkbox for it, and the owner ticks it each time.
   - "Follow-up issue" comes after the plan.
   - A preset is a subset of the steps, in order.
   - Built-in presets: **Full**, **Quick** (a plain session, no steps), **Plan only**, **Plan + Issue**. They can be edited and reset.
   - Presets live in Bobix's own settings.
8. **Start suggestions (later).** Count each ticket's real stopping step only. No score. A session that went past its preset counts as stopping at the further step.
9. **Phone v1 keeps its two starts, Quick and Full.** Presets reach the phone in v2.

### Noted consequences

These follow from the decisions above. They were stated to the owner; they are not separate questions.

- The repository's main folder sits on the branch of the issue being worked on.
- The review step stays as today: the session reviews its own change with `/code-review`. A person reading the diff belongs to the diff review screen (P3).

## 2. Phases

| Phase | What |
|-------|------|
| **Now** | Workflow presets: the list at Start, the prompt each preset writes, Settings → Workflows. The three suggestion switches stay hidden until suggestions exist. |
| | The ask-once on a file edit outside the preset. |
| | Merge approval in every preset. |
| | One session per issue in the repository folder, with the board outside the worktree. |
| | The worktree only for a second issue in the same repository. |
| | Start stops and says why when the repository folder has uncommitted files. |
| | Shortcut to the waiting session, a "done, not seen yet" mark, and a nudge when a session is stuck. |
| | A `bobix` CLI for agents (`status`, `wait`). |
| | Safe bulk cleanup of merged worktrees Bobix made: check uncommitted files and unpushed commits first. |
| **P1** | Bobix's own terminal (PTY) in Sessions. |
| | Restart an agent that exited. |
| | Work start runs there. |
| | Type into a session from the pet. |
| | Send CI failures and review comments back to Claude. The owner reads and edits the prompt before it is sent. |
| **P2** | The coordinator and workers (model item 4), including: |
| | a child copy only when two tasks touch the same files, or when the caller asks; |
| | workers report done, and the person decides merges; |
| | Claude's default permission mode, with skip-permissions only by hand and marked on the pet; |
| | a tree in Sessions and a dependency diagram. |
| **P3** | Panes, splits and tabs. |
| | A daemon that keeps terminals alive when the app closes. |
| | Diff review with line notes. |
| | Start suggestions and agent-proposed presets. |
| | Terminals on SSH machines. |
| **Phone** | v1 closed beta continues as planned. |
| | v2 comes after the terminal: chat and prompts (Face ID), read-only live terminal, diff and notes, usage, presets. |

## 3. Dropped from "Now"

The "worktrees like Orca" block of the 15 Nov plan:

- `.worktreeinclude`;
- linked folders such as `node_modules`;
- a setup script with run / ask / skip;
- starting from another branch, PR or SHA.

It matters only if worktrees become common again.

## 4. Not doing

- Linear.
- Browser / design mode.
- Mods / plugins.
- Tailscale for the phone.
- UI code sent from the desktop to the phone.
- Raw keystrokes from the phone.
- Commits from the phone.
- Other agents (Codex, opencode), until later.

## 5. Open, not decided

- How Bobix recognises the steps for suggestions: git and the forge, skill names, or also a Haiku read of prompts. If Haiku is used: does it count in the usage bar, and does it pass the redactor?
- Whether sessions in outside terminals (hook only) are counted for suggestions.
- Whether a preset applies to a whole run or to each worker.
- From the 15 Nov plan:
  - when Stop opens to the phone;
  - whether a start from the phone goes through Bobix's terminal after P1;
  - whether the Sessions mockup is approved.
