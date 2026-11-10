#!/bin/bash
# sheet.sh VIDEO OUT.jpg: a 1 frame/s contact sheet, 10 frames a row (frame n = second n, left to right, top to bottom).
ffmpeg -v error -y -i "$1" -vf "fps=1,scale=384:216,tile=10x15:padding=4:color=black" -frames:v 1 -q:v 3 "$2" && echo "$2"
