#!/usr/bin/env bash
# Generates the clip the export benchmark runs on: synthetic 1080p60 H.264 at ~50 Mbps with a
# keyframe every 2 s (what a gameplay capture looks like to the exporter) and a quiet tone.
#
#   scripts/make-bench-video.sh [out.mp4] [seconds]
#   OCA_BENCH_VIDEO=out.mp4 cargo test --release -p core --test it export_perf -- --ignored --nocapture
#
# Needs an ffmpeg with libopenh264 or libx264 (the LGPL builds the project pins have the former).
set -euo pipefail
out="${1:-bench_1080p60.mp4}"
secs="${2:-22}"
ffmpeg="${FFMPEG:-ffmpeg}"
encoder=libx264
"$ffmpeg" -hide_banner -encoders 2>/dev/null | grep -q libx264 || encoder=libopenh264
"$ffmpeg" -v error -y \
  -f lavfi -i "testsrc2=size=1920x1080:rate=60:duration=$secs" \
  -f lavfi -i "sine=frequency=440:sample_rate=48000:duration=$secs" \
  -af volume=0.05 -pix_fmt yuv420p \
  -c:v "$encoder" -b:v 50M -g 120 -c:a aac -b:a 192k -shortest "$out"
echo "wrote $out ($encoder)"
