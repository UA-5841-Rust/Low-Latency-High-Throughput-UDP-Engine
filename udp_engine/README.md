## Performance Profiling

1. Build the project with frame pointers for accurate stack traces:
   `RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release`

2. Run the server and profile with perf:
   `sudo perf record -F 99 -g -p $(pgrep udp_engine) -- sleep 20`

3. Hardware Events Analysis:
   `sudo perf stat -e L1-dcache-load-misses,LLC-load-misses,context-switches -p $(pgrep udp_engine)`

4. View FlameGraph:
   `hotspot perf.data`