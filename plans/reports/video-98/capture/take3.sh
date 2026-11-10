#!/bin/bash
# Take 3: the long-read session reads on until its context runs low (battery alarm); another asks to delete a folder.
node rec-pets.mjs "$W/clips/t3-ctx.mp4" 240 &
sleep 3; ./world.sh type bobix-85 "Keep going for #85: read each of these files in full with the Read tool, one call per file (parts of 1000 lines when it says a file is too long), skipping none: src-tauri/Cargo.lock test/core.test.js site/vi/index.html site/index.html test/overlay.test.js src/ui/pixelate.js docs/roadmap.md src-tauri/src/model.rs src/ui/gif/gifenc.js src-tauri/src/smoke.rs src-tauri/src/hooks.rs src/ui/pet-ride.js src-tauri/src/server.rs src/ui/board.js src/ui/pets-ondemand.js README.vi.md src-tauri/src/habits.rs README.md src/ui/creature.js docs/privacy.md src-tauri/src/skills.rs src/ui/costume.js test/fixtures/work-snapshot.json src-tauri/src/process.rs src-tauri/src/procs.rs src-tauri/src/overlay.rs. Then give me one line on how big the code base is. Read only."
sleep 25; ./world.sh type bobix-17 "Run exactly this and nothing else: rm -rf site"
wait; echo take3 done
