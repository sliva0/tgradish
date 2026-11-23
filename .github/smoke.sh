#!/bin/sh
# Smoke-tests a tgradish binary, as `sh smoke.sh BIN`: every release test job
# and `cargo xtask local-release` run it, so each build passes the same checks.
# POSIX sh only, as Alpine's busybox and Windows' Git Bash run it too. Writes
# its files into the current directory, overwriting those of an earlier run.
set -eux
bin=$1
"$bin" --version
"$bin" ffmpeg status
# 16x16 PNG: red square with transparent border.
echo iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAAG0lEQVR42mNgGF7gjobGf2LwqAGjBtDWgKEJAGe22M1pQ+POAAAAAElFTkSuQmCC \
  | base64 -d > input.png
"$bin" convert input.png -o still.webm --length 1 --preset fast --overwrite
# Decode VP9 with alpha to verify the libvpx decoder works.
"$bin" convert still.webm -o out.webm --preset fast --overwrite
"$bin" convert input.png -o out.tgs --overwrite
"$bin" convert input.png -o out.webp --overwrite
"$bin" inspect out.webm out.tgs out.webp
