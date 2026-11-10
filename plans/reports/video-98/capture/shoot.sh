#!/bin/bash
# shoot.sh NAME [JS]: run JS in the panel (a person's scroll or click), then capture the panel window to $W/png/NAME.png
[ -n "$2" ] && node "$(dirname "$0")/cmd-server.mjs" push panel "$2" >/dev/null && sleep ${WAIT:-1.5}
screencapture -x -o -l"$(cat "$W/panel-wid")" "$W/png/$1.png" && echo "$1"
