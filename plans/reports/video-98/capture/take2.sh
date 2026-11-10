#!/bin/bash
# Take 2, the busy morning: reading, editing, running commands, one session running out of context, two asking.
node rec-pets.mjs "$W/clips/t2-busy.mp4" 170 &
sleep 5;  ./world.sh type bobix-84 "Read src-tauri/src/state.rs, then src-tauri/src/model.rs, then src/ui/board.js, one at a time, and list in 3 bullets where a per-ticket token total would fit. Read only."
sleep 3;  ./world.sh type bobix-86 "Add 5 more bullets to notes/86-companion.md about what Bobix would show from the mod's data. Make three separate small edits."
sleep 3;  ./world.sh type bobix-88b "Run these one at a time and summarise each in a line: git log --oneline -5, git branch -a, git status, wc -l src/ui/*.js"
sleep 3;  ./world.sh type bobix-88 "Read src/ui/pets-ondemand.js and src/ui/pet-ride.js and explain in 2 bullets how a pet rides up the screen. Read only."
sleep 4;  ./world.sh type bobix-85 "Read each of these files in full, in parts of 1000 lines when a file is long, without skipping any part: src/ui/panel.html, src-tauri/src/main.rs, src/ui/i18n.js, src/ui/pets.html, src-tauri/src/work.rs, src-tauri/src/state.rs, src/ui/pet-fx.js, src-tauri/src/settings.rs. Then tell me in 3 bullets what a companion mod pane could show. Read only."
sleep 8;  ./world.sh type bobix-dev "Check that the repo page is reachable: run curl -sI https://github.com/baonguyen-agilityio/bobix | head -1"
sleep 20; ./world.sh type bobix-17 "Run exactly this and nothing else: rm docs/media/demo.gif"
wait
echo take2 done
