# Skill process file

A team can tell Bobix which Claude Code skill to suggest for which kind of issue. Write it in **`.bobix/skills.json`** at the root of the repository and commit it, so everyone who clones the repository gets the same suggestions. Bobix reads the file each time you press **Start** on an issue of that project. It never changes the file.

For a repository without one, each person can import a process in **Settings → Skill process**, from a file on their computer or from an `https://` link. Bobix keeps the imported copy in its own settings. When the repository has its own file, the repository's file is used.

The file only says which skill to suggest. In the Start box the suggested skill is preselected and marked **Suggested**, and you can still pick any other skill, or none, before anything starts. With a process, its rules replace Bobix's own guess from labels and role for that project.

## Format

```json
{
  "version": 1,
  "name": "Shop team",
  "rules": [
    { "when": { "label": ["bug", "regression"] }, "skill": "fix-ticket" },
    { "when": { "label": "enhancement", "role": "dev" }, "skill": "ak:cook" },
    { "when": { "label": "spec", "role": "ba" }, "skill": "ba-qa-kit:write-stories" },
    { "when": { "role": "qa" }, "skill": "ba-qa-kit:write-test-cases" }
  ],
  "install": {
    "ba-qa-kit:write-stories": "/plugin install ba-qa-kit@ba-qa-kit",
    "ba-qa-kit:write-test-cases": "/plugin install ba-qa-kit@ba-qa-kit"
  }
}
```

| Key | Required | What it is |
|---|---|---|
| `version` | yes | Always `1`. |
| `name` | no | A short name shown next to the process (100 characters at most). |
| `rules` | yes | 1 to 100 rules, checked in order. The first rule that fits the issue **and** names a skill Claude has for that folder is suggested. |
| `rules[].when` | yes | One or more conditions; all of them must fit. `label`: one label or a list of up to 20 (any of them on the issue fits, case does not matter). `role`: `ba`, `qa` or `dev` (your role, set in Settings). `stage`: `todo`, `doing` or `verify` (an issue you start is at `todo`). |
| `rules[].skill` | yes | The skill's command, as you would type it after `/`: `fix-ticket`, or `plugin:skill` for a plugin's skill. Letters, digits, `-`, `_`, `.` and `:` only. |
| `install` | no | For a skill some people may not have: what to tell Claude to install it (300 characters at most, one line). Without it, Bobix offers `/plugin install <plugin>` for a plugin's skill, or a sentence asking Claude to install the skill. |

When a fitting rule names a skill that Claude doesn't have, the Start box lists that skill as **not installed**, with a button that copies the install line. You paste it into Claude yourself. Bobix never installs anything and never writes to Claude's skill folders.

## What Bobix accepts

The file is data and nothing in it is ever run. Bobix refuses the whole file, and says where the problem is, when:

- it is not JSON, is larger than 64 KB, or `version` is not `1`;
- it has a key the table above doesn't list, at the top, in a rule or in `when`;
- a skill name has anything besides letters, digits, `-`, `_`, `.` and `:`, such as spaces, slashes, `;` or a new line;
- a role or stage is not one of the values above;
- a label, the name or an install line is empty, too long, or has control characters such as a new line.

A file that is refused is not used. The Start box and Settings show why.

An imported link is read once, when you press Import: one GET over `https` only (also after redirects), 64 KB and 10 seconds at most.
