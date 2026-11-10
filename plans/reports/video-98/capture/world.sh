#!/bin/bash
# world.sh setup | start | stop | clone NAME BRANCH | term NAME "PROMPT" [claude args…] | type NAME "TEXT" | clean
# The develop build (0.2.0-rc.4 + evidence patch) on port 4791 with its own data folder and Bobix home: never the
# person's own Bobix or settings. Real Claude Code sessions (CLI, in Terminal) in throwaway clones of the bobix repo,
# with Bobix's hook given by --settings and user settings left out (--setting-sources project,local), so they report
# here only and nothing is written to ~/.claude/settings.json.
W=${W:?set W to the scratch world folder}; PORT=4791
REPO=/Users/baonguyen/Desktop/test/bobix/.claude/worktrees/98-video-day-of-work-f80217
CAP=$REPO/plans/reports/video-98/capture
BIN=$REPO/src-tauri/target/release/bobix
CODE=/Users/Shared/bobix-day
case "$1" in
setup)
  mkdir -p "$W/home/.claude/bobix" "$W/data" "$W/bin" "$CODE"
  cp "$REPO/src/hook/hook.js" "$W/home/.claude/bobix/hook.js"
  # the person's own skills and plugins, read by Bobix for the skill suggestion (symlinks, read only)
  ln -sfn "$HOME/.claude/skills" "$W/home/.claude/skills"; ln -sfn "$HOME/.claude/plugins" "$W/home/.claude/plugins"
  cp "$CAP/gh-bobix.js" "$W/bin/gh"; chmod +x "$W/bin/gh"
  printf '#!/bin/sh\ncase "$2" in *" gh") echo "%s/bin/gh" ;; *" glab") ;; *) exec /bin/zsh "$@" ;; esac\n' "$W" > "$W/bin/sh-stand-in"; chmod +x "$W/bin/sh-stand-in"
  [ -f "$W/data/settings.json" ] || cat > "$W/data/settings.json" <<J
{ "language": "en", "port": $PORT, "github": true, "gitlab": false, "role": "dev", "size": 64, "label": "both",
  "approveWait": -1, "longWait": 2, "stuckNoPr": 1, "stuckIdle": 1, "stuckBranch": 1, "habitsOn": true,
  "notifyWaiting": false, "notifyDone": false, "sound": false, "autostart": false, "checkUpdates": false,
  "guideSeen": true, "tourSeen": ["pets","ondemand","dnd","board","stages","stuck","next","skill","skills","standup","wait","allow"],
  "seenVersion": "0.2.0-rc.4", "petsVisible": true, "showIdle": true, "sleepHide": 60, "fadeNearCursor": false }
J
  printf '{"hooks":{%s}}\n' "$(for e in SessionStart UserPromptSubmit PreToolUse PostToolUse PermissionRequest Notification Stop SessionEnd; do
    printf '"%s":[{"hooks":[{"type":"command","command":"node %s/home/.claude/bobix/hook.js %s","timeout":600}]}],' $e "$W" $PORT; done | sed 's/,$//')" > "$W/claude-hooks.json"
  echo ok ;;
clone) # NAME BRANCH: a throwaway clone (objects shared with the repo, its own refs), origin = the GitHub repo, never pushed
  d=$CODE/$2; [ -d "$d" ] && { echo exists; exit; }
  git clone -q --shared /Users/baonguyen/Desktop/test/bobix "$d" && git -C "$d" remote set-url origin git@github.com:baonguyen-agilityio/bobix.git
  git -C "$d" remote set-url --push origin no-push; git -C "$d" checkout -q -B "$3" origin/develop 2>/dev/null || git -C "$d" checkout -q -b "$3"
  git -C "$d" branch --unset-upstream 2>/dev/null; echo "$d on $3" ;;
start)
  ( unset CLAUDE_PID CLAUDE_CODE_HOST_SESSION_ID CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID
    EVIDENCE_PANEL_W=${PW:-1440} EVIDENCE_PANEL_H=${PH:-900} EVIDENCE_TAB=${TAB:-sessions} EVIDENCE_PANEL_JS="$W/panel.js" EVIDENCE_PETS_JS="$W/pets.js" \
    BOBIX_PORT=$PORT BOBIX_DATA_DIR="$W/data" BOBIX_HOME="$W/home" SHELL="$W/bin/sh-stand-in" exec "$BIN" ) >> "$W/app.log" 2>&1 &
  echo $! > "$W/app.pid"
  for i in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/ui/theme.css" && break; sleep 0.2; done
  echo "app $(cat "$W/app.pid") on $PORT" ;;
show) curl -s -X POST -H "x-bobix: $(cat "$W/home/.claude/bobix/token")" -H 'content-type: application/json' -d '{"version":"0"}' "http://127.0.0.1:$PORT/show" -o /dev/null -w "show %{http_code}\n" ;;
stop) [ -f "$W/app.pid" ] && kill "$(cat "$W/app.pid")" 2>/dev/null; rm -f "$W/app.pid"; sleep 1 ;;
term) # NAME PROMPT [args]: a real interactive Claude Code session in a new Terminal window, in that clone
  n=$2; p=$3; shift 3
  cmd="cd $CODE/$n && clear && claude --setting-sources project,local --settings $W/claude-hooks.json --model haiku $* $(printf %q "$p")"
  osascript -e 'on run a' -e 'tell application "Terminal"' -e 'set t to do script (item 1 of a)' -e 'return id of (first window whose tabs contains t)' -e 'end tell' -e 'end run' "$cmd" > "$W/term-$n" && echo "term $n window $(cat "$W/term-$n")" ;;
type) # NAME TEXT: type into that session's Terminal tab (TEXT + Return), as a person would
  osascript -e 'on run a' -e 'tell application "Terminal" to do script (item 2 of a) in tab 1 of window id ((item 1 of a) as integer)' -e 'end run' "$(cat "$W/term-$2")" "$3" >/dev/null ;;
esac
