#!/usr/bin/env bash
set -euo pipefail

# Start the engine first, then run: sudo ./scripts/profile.sh <pid> [seconds]
PID="${1:?usage: $0 <udp_engine_pid> [seconds]}"
SECONDS="${2:-20}"
mkdir -p flamegraphs results

sudo perf stat -p "$PID" -e cycles,instructions,context-switches,L1-dcache-load-misses,LLC-load-misses -- sleep "$SECONDS" \
  2> results/perf-stat.txt
sudo perf record -F 99 -g -p "$PID" -- sleep "$SECONDS"
mv perf.data flamegraphs/perf.data
echo "Open flamegraphs/perf.data in Hotspot, or run: hotspot flamegraphs/perf.data"
