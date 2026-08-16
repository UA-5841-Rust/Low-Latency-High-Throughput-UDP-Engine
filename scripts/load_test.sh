#!/usr/bin/env bash
set -euo pipefail

# Run this on a separate host where possible, otherwise loopback is supported.
HOST="${1:-127.0.0.1}"
PORT="${2:-9000}"
THREADS="${3:-4}"
SECONDS="${4:-20}"
exec ./target/release/udp_loadgen --host "$HOST" --port "$PORT" \
  --threads "$THREADS" --size 64 --seconds "$SECONDS"
