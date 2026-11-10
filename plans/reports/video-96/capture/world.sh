#!/bin/bash
# world.sh start [tab] | send <scene> | stop
# The real rc.4 build on port 4791 with a scratch home + data folder (never the person's own Bobix or ~/.claude).
E=$(cd "$(dirname "$0")" && pwd); PORT=4791
REPO=/Users/baonguyen/Desktop/test/bobix/.claude/worktrees/96-intro-video-1367cd
BIN=${BIN:-$REPO/src-tauri/target/release/bobix}
C=/Users/Shared/acme
send() { # sid cwd json-extra [title]
  local t="$E/tr/$1.jsonl"; [ -f "$t" ] || printf '{"type":"ai-title","aiTitle":"%s"}\n' "$4" > "$t"
  echo "{\"session_id\":\"$1\",\"cwd\":\"$2\",\"transcript_path\":\"$t\",$3}" | HOME=$E/home CLAUDE_PID=$(cat $E/sleep.pid) node "$REPO/src/hook/hook.js" $PORT >/dev/null; }
ask() { # sid cwd command: a permission request the hook holds until answered
  ( echo "{\"session_id\":\"$1\",\"cwd\":\"$2\",\"transcript_path\":\"$E/tr/$1.jsonl\",\"hook_event_name\":\"PermissionRequest\",\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"$3\"}}" \
    | HOME=$E/home CLAUDE_PID=$(cat $E/sleep.pid) node "$REPO/src/hook/hook.js" $PORT >/dev/null 2>&1 & ) ; }
S1=7a1f0c2e-1111-4a8e-9a01-000000000001 S2=7a1f0c2e-2222-4a8e-9a01-000000000002 S3=7a1f0c2e-3333-4a8e-9a01-000000000003
S4=7a1f0c2e-4444-4a8e-9a01-000000000004 S5=7a1f0c2e-5555-4a8e-9a01-000000000005
case "$1" in
start)
  "$0" stop; rm -rf "$E/data" "$E/tr"; mkdir -p "$E/data" "$E/tr"
  cat > "$E/data/settings.json" <<J
{ "language": "vi", "github": true, "gitlab": false, "role": "dev", "port": $PORT,
  "skillPins": { "ak:cook": ["dev"], "review-diff": ["dev", "qa"] },
  "guideSeen": true, "tourSeen": ["pets","ondemand","dnd","board","stages","stuck","next","skill","skills","standup","wait","allow"], "seenVersion": "0.2.0-rc.4",
  "petsVisible": true, "zoneStart": 70, "zoneEnd": 100, "autostart": false, "checkUpdates": false, "sound": false }
J
  ( unset CLAUDE_PID CLAUDE_CODE_HOST_SESSION_ID CLAUDECODE CLAUDE_CODE_ENTRYPOINT; cd "$E"
    EVIDENCE_PANEL_W=${PW:-1440} EVIDENCE_PANEL_H=${PH:-900} EVIDENCE_TAB=${2:-sessions} EVIDENCE_JS="$(cat $E/${JSF:-none.js} 2>/dev/null)" \
    BOBIX_PORT=$PORT BOBIX_DATA_DIR=$E/data BOBIX_HOME=$E/home HOME=$E/home SHELL=$E/bin/sh-stand-in exec "$BIN" ) > "$E/app.log" 2>&1 &
  echo $! > "$E/app.pid"
  for i in $(seq 50); do curl -s -o /dev/null "http://127.0.0.1:$PORT/ui/theme.css" && break; sleep 0.2; done
  sleep 7200 & echo $! > "$E/sleep.pid"
  echo "app $(cat $E/app.pid) on $PORT" ;;
show) curl -s -X POST -H "x-bobix: $(cat $E/home/.claude/bobix/token)" -H 'content-type: application/json' -d '{"version":"0"}' "http://127.0.0.1:$PORT/show" -o /dev/null -w "show %{http_code}\n" ;;
board) # a working day: two coding, one asking, one done, one asleep
  send $S1 "$C/shop-web" '"hook_event_name":"SessionStart","source":"startup"' "Mã giảm giá khi thanh toán"
  send $S1 "$C/shop-web" '"hook_event_name":"UserPromptSubmit","prompt":"/ak:cook https://github.com/acme/shop-web/issues/41"'
  send $S1 "$C/shop-web" '"hook_event_name":"PreToolUse","tool_name":"Edit","tool_input":{"file_path":"'$C'/shop-web/src/checkout/coupon.ts"}'
  send $S2 "$C/shop-web-58" '"hook_event_name":"SessionStart","source":"startup"' "Địa chỉ giao hàng mặc định"
  send $S2 "$C/shop-web-58" '"hook_event_name":"UserPromptSubmit","prompt":"Làm #58: cho khách chọn địa chỉ mặc định"'
  send $S2 "$C/shop-web-58" '"hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{"file_path":"'$C'/shop-web-58/src/account/addresses.ts"}'
  send $S3 "$C/notes" '"hook_event_name":"SessionStart","source":"startup"' "Ghi chú họp nhóm"
  send $S3 "$C/notes" '"hook_event_name":"UserPromptSubmit","prompt":"Tóm tắt buổi họp thành ba việc cần làm"'
  send $S3 "$C/notes" '"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"npm run build"}'
  send $S5 "$C/docs" '"hook_event_name":"SessionStart","source":"startup"' "Trang tài liệu"
  send $S4 "$C/shop-api" '"hook_event_name":"SessionStart","source":"startup"' "Giới hạn đặt lại mật khẩu"
  send $S4 "$C/shop-api" '"hook_event_name":"UserPromptSubmit","prompt":"/ak:fix https://github.com/acme/shop-api/issues/12"'
  send $S4 "$C/shop-api" '"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"npm run lint"}'
  ;;
ask4) ask $S4 "$C/shop-api" "npm test -- password-reset" ;;
done3) send $S3 "$C/notes" '"hook_event_name":"Stop"' ;;
tool) send $2 "$3" '"hook_event_name":"PreToolUse","tool_name":"'$4'","tool_input":{"command":"x","file_path":"'$3'/src/app.ts"}' ;;
stop)
  [ -f "$E/app.pid" ] && kill "$(cat $E/app.pid)" 2>/dev/null; [ -f "$E/sleep.pid" ] && kill "$(cat $E/sleep.pid)" 2>/dev/null
  rm -f "$E/app.pid" "$E/sleep.pid"; sleep 1 ;;
esac
