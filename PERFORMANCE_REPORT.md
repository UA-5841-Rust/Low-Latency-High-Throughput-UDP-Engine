# Performance report

This repository supplies a reproducible measurement workflow, but deliberately
does not invent benchmark numbers. Fill this report with measurements from the
target Linux machine and attach the two `perf.data`/Hotspot exports.

## Test metadata

| Field | Value |
|---|---|
| Date / commit | 2026-08-16 / 5ccad2d |
| CPU / NIC / kernel | WSL2 VM CPU / loopback (127.0.0.1) / Linux 6.6.87.2-microsoft-standard-WSL2 |
| Isolated CPUs | Engine: `2-9`, Loadgen: `10-15` |
| Packet size / source hosts | _fill packet size_ / source: local host (WSL2 loopback) |
| Engine command | `taskset -c 2-9 ./target/release/udp_engine --bind 0.0.0.0 --port 9000 --workers 4 --cpu 2` |

## Results

| Variant | PPS | p50 (us) | p90 (us) | p99 (us) | p99.9 (us) | max (us) | drops |
|---|---:|---:|---:|---:|---:|---:|---:|
| Baseline (single `recvfrom`, unpinned) | _not measured in this run_ | _ | _ | _ | _ | _ | _ |
| Optimized (`recvmmsg`, reuseport, pinned) | 2,001,887 (avg over 30s), total=60,056,615 | 4 | 8 | 16 | 65–131 | 2,600–6,534 | 0 |

Notes:
- Loadgen per-second send rate ranged approximately `1.93M .. 2.08M pps`.
- Engine logs showed periodic lines with `pps=0` while latency and drops continued updating; treat PPS from loadgen as authoritative for this run.

## `perf stat`

| Variant | context-switches | L1-dcache-load-misses | LLC-load-misses |
|---|---:|---:|---:|
| Baseline | _not measured in this run_ | _ | _ |
| Optimized | 57 (20.008s window) | 6,603,401 | not supported in WSL2 |

Additional optimized counters (same 20s run):
- cycles: `345,460,220,388`
- instructions: `226,966,593,526`
- IPC: `0.66`
- cache-references: `7,922,318`
- cache-misses: `885,579` (`11.18%` of cache refs)
- L1-dcache-loads: `57,105,936,096`
- L1-dcache-load-miss rate: `0.01%`

## FlameGraphs

- Baseline: `flamegraphs/baseline-perf.data` (attach Hotspot SVG/PNG export)
- Optimized: `flamegraphs/optimized-perf.data` (attach Hotspot SVG/PNG export)

Interpretation:
- The optimized path sustains ~2.0 Mpps on this WSL2 setup with zero observed drops in sampled logs.
- Latency percentiles are low (p50~4us, p99~16us), with occasional max spikes (2.6–6.5 ms), which are plausible under virtualization/scheduler noise.
- WSL2 PMU support is partial; LLC counters are unavailable (`not supported`).