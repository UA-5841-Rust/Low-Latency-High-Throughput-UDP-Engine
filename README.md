# Topics:
* Building projects (GCC/Clang, Makefiles).
* Debugging with `gdb`.
* Performance profiling (`perf`, `gprof`, `Hotspot`).
* Binding to CPU cores (`taskset`, `isolcpus`) and NUMA architecture.


# Practical task 1:
* Write a high-load UDP server in Rust/C.
* Perform load profiling using `perf record`, build a FlameGraph in `Hotspot`, and optimize the server by binding threads to isolated cores (`taskset`).

# Practical task 2: Building a High-Throughput Low-Latency UDP Engine in Rust

**Topic:** Low-level network stack optimization, Linux system calls, Zero-Allocation, Lock-Free architecture, and system profiling.


**Stack:** Rust, Linux Kernel API (`libc` / `io_uring`), `perf`, `Hotspot`, `sysctl`

---

## Objective

Design and build a high-performance UDP server in Rust capable of processing **5,000,000 to 10,000,000+ packets per second (PPS)** on a single port with a $p_{99.9}$ latency of $< 50\ \mu\text{s}$ without relying on high-level asynchronous frameworks (such as Tokio or Actix).

> **Why this matters:** The standard `Arc<UdpSocket>` + `recvfrom` approach creates heavy socket mutex contention inside the Linux kernel and generates millions of individual system calls, leading to extreme CPU context-switching overhead.

---

## 1. Architectural & Technical Requirements

### 1.1. Eliminating Kernel Contention (`SO_REUSEPORT`)

* Sharing a single socket across threads is strictly prohibited.
* Each worker thread must create and bind its **own UDP socket** to the same IP/Port using the `SO_REUSEPORT` socket option.
* The Linux kernel will distribute incoming traffic via 4-tuple hashing (`src_ip`, `src_port`, `dst_ip`, `dst_port`) without inter-thread locks.

### 1.2. System Call Batching (`recvmmsg` / `io_uring`)

* Issuing single `recvfrom` system calls is prohibited.
* Implement packet reading in batches (32–64 packets per syscall):
* **Option A:** Direct `libc::recvmmsg` system calls.
* **Option B:** Asynchronous packet batching via **`io_uring`** (using low-level `io-uring` bindings).



### 1.3. Memory & Cache Management (Zero-Allocation)

* **Zero-Allocation in the hot path:** Dynamic heap allocations (`Box::new`, `Vec::push`, `String`) during packet processing are forbidden. All buffers must be pre-allocated at application startup.
* **Cache Line Alignment:**
* Per-thread data structures must be aligned to CPU cache line boundaries (`#[repr(align(64))]`).
* Atomic counters owned by different threads must not share a cache line to prevent *Cache Line Bouncing (False Sharing)*.



### 1.4. Lock-Free Inter-Thread Communication

* Receiver threads pass processed packets to worker threads via a **Lock-Free SPSC (Single Producer Single Consumer)** ring buffer.
* Using `std::sync::Mutex` or `std::sync::RwLock` on the execution path is forbidden.

---

## 2. Test Environment Setup

To achieve maximum throughput, the Linux kernel network stack must be tuned. Include a `setup_env.sh` script in your repository:

```bash
#!/bin/bash
# 1. Increase Linux kernel socket buffer sizes
sudo sysctl -w net.core.rmem_max=134217728
sudo sysctl -w net.core.rmem_default=67108864
sudo sysctl -w net.core.netdev_max_backlog=250000

# 2. Enable SO_BUSY_POLL (reduces network card interrupt latency)
sudo sysctl -w net.core.busy_read=50
sudo sysctl -w net.core.busy_poll=50

# 3. Pin processes to specific CPU cores upon launch
# (Isolate cores using isolcpus in the GRUB bootloader if needed)

```

---

## 3. Metrics & Profiling

Project results must be verified and benchmarked using Linux system utilities.

1. **Latency Percentiles:**
* Record processing latencies: $p_{50}$, $p_{90}$, $p_{99}$, $p_{99.9}$, and Maximum (use `hdrhistogram` or a custom lock-free histogram).


2. **Hardware Event Analysis (`perf stat`):**
* Measure CPU performance counters:
* `L1-dcache-load-misses`
* `LLC-load-misses` (Last Level Cache)
* `context-switches` (should approach 0 after CPU pinning)




3. **Visualization (`Hotspot`):**
* Generate and submit two FlameGraphs in your report (before and after optimization):
```bash
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --release
sudo perf record -F 99 -g -p $(pgrep udp_engine) -- sleep 20
hotspot perf.data

```





---

## Grading Rubric

| Criterion | Requirements |
| --- | --- |
| **`SO_REUSEPORT` + CPU Pinning** | Each thread owns a distinct socket queue and is strictly pinned to a dedicated core via `core_affinity` or `taskset`. |
| **Syscall Batching (`recvmmsg` / `io_uring`)** | Packets are read in batches, minimizing system call overhead. |
| **Zero-Allocation & Alignment** | No heap allocations in the hot loop; proper usage of `#[repr(align(64))]`. |
| **Lock-Free SPSC Ring Buffer** | Inter-thread data passing without OS mutexes or spinlocks. |
| **Profiling & Performance Report** | Comprehensive report featuring $p_{99.9}$ latencies, `perf stat` comparisons, and FlameGraphs from `Hotspot`. |

---

## Bonus

* **SIMD Parsing:** Implement SIMD-accelerated header validation or checksum calculation (CRC32) using `AVX2` or `NEON` instructions.
* **eBPF/XDP:** Write a basic XDP program to filter out malformed UDP packets directly at the network driver level (before entering the Linux network stack).

---

## Submission Requirements

1. **Source Code:** Link to a public or private GitHub repository.
2. **Documentation (`README.md`):**
* Build and execution instructions.
* Traffic generation and benchmarking scripts.
* A **"Performance Report"** section containing PPS/Latency tables, FlameGraph screenshots, and cache-miss analysis.
