#!/bin/bash
# Scratch world for the evidence run: home (skills, plugins), code (git repos), bin (stand-in gh, shell), data.
set -e
CODE=/Users/Shared/acme
E=$(cd "$(dirname "$0")" && pwd); rm -rf "$E/home" "$CODE" "$E/data"; mkdir -p "$E/home" "$CODE" "$E/data" "$E/bin"
H=$E/home/.claude
sk() { mkdir -p "$1/$2"; printf -- "---\nname: %s\ndescription: %s\n---\nBody.\n" "$2" "$3" > "$1/$2/SKILL.md"; }
sk $H/skills review-diff "Review the current diff for bugs, risky changes and missing tests"
sk $H/skills write-pr-description "Write a pull request description from the branch's commits"
sk $H/skills release-notes "Draft release notes from merged pull requests"
sk $H/skills standup-notes "Turn yesterday's commits into stand-up notes"
sk $H/skills sql-explain "Explain a slow SQL query and suggest an index"
mkdir -p $H/skills/synced/acc1; sk $H/skills/synced/acc1 docx "Create and edit Word documents"; sk $H/skills/synced/acc1 pdf "Read, fill and merge PDF files"
P=$E/home/plugins/ak; mkdir -p $P/.claude-plugin; echo '{"name":"ak"}' > $P/.claude-plugin/plugin.json
sk $P/skills cook "Implement a feature from a plan, with tests"; sk $P/skills fix "Fix a bug: find the cause first, then the smallest fix"; sk $P/skills plan "Plan a feature in phases with acceptance criteria"; sk $P/skills debug "Debug a failing test or error step by step"
mkdir -p $H/plugins; printf '{"version":2,"plugins":{"ak@agentkit":[{"scope":"user","installPath":"%s"}]}}' "$P" > $H/plugins/installed_plugins.json
repo() { # name remote branch
  local d=$CODE/$1; mkdir -p $d; git -C $d init -q -b main; git -C $d config user.email dev@example.com; git -C $d config user.name Dev
  echo "# $1" > $d/README.md; git -C $d add -A; git -C $d commit -qm init; git -C $d remote add origin "$2"
  [ -n "$3" ] && git -C $d checkout -qb "$3"; true; }
repo shop-web git@github.com:acme/shop-web.git feat/41-checkout-coupons
repo shop-api git@github.com:acme/shop-api.git 12-rate-limit-reset
git -C $CODE/shop-web worktree add -q -b 58-default-address $CODE/shop-web-58 main
sk $CODE/shop-web/.claude/skills fix-ticket "Fix one ticket end to end: reproduce, fix, test, PR"
sk $CODE/shop-web/.claude/skills shop-e2e "Run the shop's end-to-end checkout tests"
sk $CODE/shop-api/.claude/skills api-contract-check "Check API changes against the OpenAPI contract"
mkdir -p $CODE/shop-web/.bobix; cat > $CODE/shop-web/.bobix/skills.json <<'J'
{ "version": 1, "name": "Shop team",
  "rules": [ { "when": { "label": ["bug", "regression"] }, "skill": "fix-ticket" },
             { "when": { "label": "enhancement", "role": "dev" }, "skill": "ak:cook" },
             { "when": { "label": "spec", "role": "ba" }, "skill": "ba-qa-kit:write-stories" },
             { "when": { "role": "qa" }, "skill": "ba-qa-kit:write-test-cases" } ],
  "install": { "ba-qa-kit:write-stories": "/plugin install ba-qa-kit@ba-qa-kit", "ba-qa-kit:write-test-cases": "/plugin install ba-qa-kit@ba-qa-kit" } }
J
mkdir -p $CODE/notes
cat > $E/bin/sh-stand-in <<S
#!/bin/sh
case "\$2" in *" gh") echo "$E/bin/gh" ;; *" glab") ;; *) exec /bin/zsh "\$@" ;; esac
S
chmod +x $E/bin/sh-stand-in
echo ok
