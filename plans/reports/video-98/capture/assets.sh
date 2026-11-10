#!/bin/bash
# assets.sh: cut the recordings and panel shots in $W into source/assets (clips trimmed and cropped, costume tiles, stills).
set -e
A=$(cd "$(dirname "$0")/.." && pwd)/source/assets; C=$W/clips; P=$W/png; mkdir -p "$A"
cp -R "$(dirname "$0")/../../video-96/source/assets/fonts" "$A/" ; cp "$(dirname "$0")/../../video-96/source/assets/icon.png" "$A/"
v() { ffmpeg -v error -y "$@" -an -c:v libx264 -crf 18 -pix_fmt yuv420p -movflags +faststart; }
v -ss 4 -t 20 -i "$C/t2-busy.mp4" "$A/pets-busy.mp4"                                         # morning: everyone at work
v -ss 172 -t 14 -i "$C/t3-ctx.mp4" -vf "crop=960:390:0:0,scale=1920:780:flags=neighbor" "$A/pets-battery.mp4"   # #85 runs low
v -ss 0 -t 12 -i "$C/t4-rest.mp4" "$A/pets-rest.mp4"                                        # evening: resting
v -i "$C/ov-allow.mp4" -vf "crop=740:464:2340:0,scale=1480:928:flags=neighbor" "$A/ov-allow.mp4"          # Allow on the pet
v -i "$C/ov-card85.mp4" -ss 2 -t 6 -vf "crop=740:464:780:0,scale=1480:928:flags=neighbor" "$A/ov-card85.mp4" # #85's card: context 14%
v -i "$C/ov-ondemand.mp4" -vf "crop=1920:1080:1520:242" "$A/ov-ondemand.mp4"                # show when needed
f() { ffmpeg -v error -y -ss "$2" -i "$C/$1" -frames:v 1 -vf "crop=300:250:$3:$4,scale=600:500:flags=neighbor" "$A/c-$5.png"; }
f t2-busy.mp4 12 1050 140 read; f t2-busy.mp4 12 775 140 bash; f t2-busy.mp4 16 1272 140 edit
f t2-busy.mp4 50 765 140 wait; f t2-busy.mp4 8 550 140 done; f t4-rest.mp4 6 700 140 rest
cp "$P"/*.png "$A/"
ffmpeg -v error -y -i "$P/od-pop.png" -vf "crop=900:500:2540:823" "$A/od-pop-crop.png"
ffmpeg -v error -y -i "$P/od-dnd.png" -vf "crop=900:300:2540:1023" "$A/od-dnd-crop.png"
ls "$A" | wc -l
