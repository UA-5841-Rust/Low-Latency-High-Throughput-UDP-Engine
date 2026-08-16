#!/usr/bin/env bash
set -euo pipefail

# Usage: ./scripts/run_pinned.sh [first_cpu] [workers] [port]
FIRST_CPU="${1:-2}"
WORKERS="${2:-1}"
PORT="${3:-9000}"
CPU_END=$(( FIRST_CPU + WORKERS * 2 - 1 ))

exec taskset -c "${FIRST_CPU}-${CPU_END}" ./target/release/udp_engine \
  --cpu "${FIRST_CPU}" --workers "${WORKERS}" --port "${PORT}"
