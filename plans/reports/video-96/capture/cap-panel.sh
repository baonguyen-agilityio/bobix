#!/bin/bash
# cap-panel.sh: one run of the scratch app; the sequencer clicks Install, then the tabs; each view captured from its window.
E=$(cd "$(dirname "$0")" && pwd); cd "$E"; mkdir -p shots
JSF=seq.js ./world.sh start setup; ./world.sh show; T0=$(date +%s); W=
w() { while [ $(( $(date +%s) - T0 )) -lt $1 ]; do sleep 0.2; done; }
cap() { [ -z "$W" ] && W=$(osascript -l JavaScript wins.js $(cat app.pid) | awk '$2=="Bobix" && $3=="1440"{print $1; exit}'); screencapture -x -o -l$W shots/$1.png && echo $1; }
w 4; cap setup-before
w 10; cap setup-after; ./world.sh board
w 13; ./world.sh ask4
w 19; cap board
w 21; ./world.sh done3
w 24; cap board-2
w 29; cap skills
w 36; cap settings
