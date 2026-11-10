#!/bin/bash
# Take 1, the busy morning: every session gets real work while the pets are recorded.
node rec-pets.mjs "$W/clips/t1-busy.mp4" 150 &
sleep 6
./world.sh type bobix-84 "Now read src-tauri/src/model.rs and say in 2 bullets where a per-ticket token total could live. Read only."
sleep 3
./world.sh type bobix-88 "Find the Start button handler in src/ui/panel.html and summarise it in 2 bullets. Read only."
sleep 3
./world.sh term bobix-dev "Save the output of node --version to node-version.txt with a shell redirect." --permission-mode default
sleep 4
./world.sh type bobix-86 "Write a short design note for #86 to notes/86-companion.md: 5 bullets on what the companion mod would send to Bobix. Create the folder."
sleep 3
./world.sh type bobix-88b "Run git log --oneline -8 -- src/ui/board.js and tell me in one line what changed on the board lately."
sleep 3
./world.sh term bobix-85 @big
sleep 6
./world.sh term bobix-17 "Issue #17: docs/media/demo.gif is no longer used. Delete it with rm." --permission-mode default
wait
echo take1 done
