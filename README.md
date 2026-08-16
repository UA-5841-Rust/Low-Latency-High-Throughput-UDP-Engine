# Low-Latency, High-Throughput UDP Engine

Linux-only Rust UDP engine for the practical tasks: a high-load server plus a
low-level batched architecture. It uses no Tokio/Actix and has no mutex on the
packet execution path. The Linux ABI bindings are deliberately minimal and
in-tree, so the project builds without downloading crates.

## Design

Each receiver owns its own UDP socket bound to the same IP/port with
`SO_REUSEPORT`; there is no shared `UdpSocket`. The kernel distributes flows
between sockets. A receiver is pinned to a CPU, receives up to 64 datagrams per
`recvmmsg` syscall into startup-allocated buffers, performs an allocation-free
checksum, and publishes `PacketMeta` records to exactly one consumer through a
bounded lock-free SPSC ring. The paired consumer is pinned to its own CPU and
records an allocation-free logarithmic latency histogram.

`head`, `tail`, and per-worker atomic statistics are 64-byte aligned to prevent
false sharing. If a consumer cannot keep up, the receiver drops metadata rather
than blocking, and reports the drop count.

## Build

Requires a Linux host with Rust stable and a kernel providing `recvmmsg` and
`SO_REUSEPORT` (modern Linux). Windows builds expose a clear unsupported-platform
message; benchmark results must be collected on Linux.

```bash
cargo test
RUSTFLAGS="-C target-cpu=native -C force-frame-pointers=yes" cargo build --release
chmod +x scripts/*.sh
sudo ./scripts/setup_env.sh
```

## Run

Reserve two logical CPUs per `--workers`: one receiver and one consumer.

```bash
# 2 receiver/consumer pairs on CPUs 2-5, port 9000
./scripts/run_pinned.sh 2 2 9000

# Or without taskset (the program still calls sched_setaffinity):
./target/release/udp_engine --bind 0.0.0.0 --port 9000 --workers 2 --cpu 2
```

`SO_REUSEPORT` hashes flows, so load testing needs multiple generator sockets
(the `--threads` option does this). Prefer a separate generator host/NIC over
loopback for representative throughput.

```bash
./scripts/load_test.sh 192.0.2.10 9000 8 30
```

Server output reports instantaneous PPS, approximate p50/p90/p99/p99.9 queue
latencies and drops each second. The histogram is log2-bucketed: it is
allocation-free and intentionally approximate; use a calibrated external probe
when strict end-to-end latency precision is needed.

## Profiling

Run the engine and load generator, then in another terminal:

```bash
sudo ./scripts/profile.sh "$(pgrep -n udp_engine)" 20
hotspot flamegraphs/perf.data
```

For the before/after comparison, keep a baseline implementation/measurement
outside the optimized binary and record it with the same generator, packet size,
duration, CPU isolation and kernel settings. Store counters and Hotspot exports
in [PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md).

## Safety and limits

The supplied sysctl script changes running kernel parameters and needs `sudo`.
CPU isolation (`isolcpus`, `nohz_full`, `rcu_nocbs`) is a bootloader policy and
is intentionally not changed automatically. The advertised PPS/latency targets
depend on hardware, NIC queues, IRQ/RPS placement, packet size and traffic flow;
they must be demonstrated with the supplied profiling workflow rather than
assumed from the code.
