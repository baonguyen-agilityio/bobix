#!/bin/bash
# ov-rec.sh OUT.mp4 SECONDS [CROP]: the real pets window (the app's overlay, alpha) captured back to back with
# screencapture (about 7 a second), laid on the video's paper colour and encoded at 30 fps keeping each frame's time.
# CROP = ffmpeg crop "w:h:x:y" in window pixels.
OUT=$1; SECS=$2; CROP=${3:-iw:ih:0:0}
WID=$(osascript -l JavaScript "$(dirname "$0")/wins.js" "$(cat "$W/app.pid")" | awk '$2=="Bobix" && $3=="pets"{print $1}')
F=$W/ovf-$(basename "$OUT"); rm -rf "$F"; mkdir -p "$F"; : > "$F/t"
end=$(( $(date +%s) + SECS )); i=0
while [ "$(date +%s)" -lt $end ]; do n=$(printf %05d $i); screencapture -x -o -l"$WID" "$F/$n.png"; python3 -c 'import time;print(time.time())' >> "$F/t"; i=$((i+1)); done
python3 - "$F" <<'P'
import sys, os
f = sys.argv[1]; t = [float(x) for x in open(f + '/t').read().split()]
with open(f + '/list.txt', 'w') as o:
    o.write('ffconcat version 1.0\n')
    for i, s in enumerate(t):
        o.write(f"file '{f}/{i:05d}.png'\nduration {((t[i+1] if i + 1 < len(t) else s + 0.14) - s):.4f}\n")
    o.write(f"file '{f}/{len(t)-1:05d}.png'\n")
P
ffmpeg -v error -y -f concat -safe 0 -i "$F/list.txt" -f lavfi -i "color=c=0xf6f0e6:s=3440x1440" \
  -filter_complex "[1][0]scale2ref[bg][fg];[bg][fg]overlay=shortest=1,crop=$CROP,fps=30,format=yuv420p,scale=trunc(iw/2)*2:trunc(ih/2)*2" -c:v libx264 -crf 16 -movflags +faststart "$OUT"
echo "$OUT $i frames"; rm -rf "$F"
