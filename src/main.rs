#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("udp_engine uses Linux recvmmsg/SO_REUSEPORT and must run on Linux.");
}

#[cfg(target_os = "linux")]
mod linux {
    use std::env;
    use std::io;
    use std::mem::{self, MaybeUninit};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use udp_engine::ring::SpscRing;
    use udp_engine::{BATCH_SIZE, MAX_DATAGRAM, PacketMeta, RING_CAPACITY, packet_checksum};

    mod sys {
        use core::ffi::{c_int, c_void};
        pub const AF_INET: c_int = 2;
        pub const SOCK_DGRAM: c_int = 2;
        pub const SOCK_CLOEXEC: c_int = 0o2000000;
        pub const SOL_SOCKET: c_int = 1;
        pub const SO_REUSEADDR: c_int = 2;
        pub const SO_RCVBUF: c_int = 8;
        pub const SO_REUSEPORT: c_int = 15;
        pub const CLOCK_MONOTONIC: c_int = 1;

        #[repr(C)]
        pub struct in_addr {
            pub s_addr: u32,
        }

        #[repr(C)]
        pub struct sockaddr_in {
            pub sin_family: u16,
            pub sin_port: u16,
            pub sin_addr: in_addr,
            pub sin_zero: [u8; 8],
        }

        #[repr(C)]
        #[derive(Clone, Copy)]
        pub struct iovec {
            pub iov_base: *mut c_void,
            pub iov_len: usize,
        }

        #[repr(C)]
        pub struct msghdr {
            pub msg_name: *mut c_void,
            pub msg_namelen: u32,
            pub msg_iov: *mut iovec,
            pub msg_iovlen: usize,
            pub msg_control: *mut c_void,
            pub msg_controllen: usize,
            pub msg_flags: c_int,
        }

        #[repr(C)]
        pub struct mmsghdr {
            pub msg_hdr: msghdr,
            pub msg_len: u32,
        }

        #[repr(C)]
        pub struct timespec {
            pub tv_sec: i64,
            pub tv_nsec: i64,
        }

        #[repr(C)]
        pub struct cpu_set_t {
            pub bits: [u64; 16],
        }

        unsafe extern "C" {
            pub fn socket(domain: c_int, type_: c_int, protocol: c_int) -> c_int;
            pub fn close(fd: c_int) -> c_int;
            pub fn setsockopt(
                fd: c_int,
                level: c_int,
                optname: c_int,
                optval: *const c_void,
                optlen: u32,
            ) -> c_int;
            pub fn bind(fd: c_int, addr: *const c_void, addrlen: u32) -> c_int;
            pub fn recvmmsg(
                fd: c_int,
                msgvec: *mut mmsghdr,
                vlen: u32,
                flags: c_int,
                timeout: *mut timespec,
            ) -> c_int;
            pub fn sched_setaffinity(
                pid: c_int,
                cpusetsize: usize,
                mask: *const cpu_set_t,
            ) -> c_int;
            pub fn clock_gettime(clockid: c_int, tp: *mut timespec) -> c_int;
        }
    }

    const DEFAULT_PORT: u16 = 9000;
    const DEFAULT_WORKERS: usize = 1;

    #[repr(align(64))]
    struct WorkerStats {
        received: AtomicU64,
        dropped: AtomicU64,
        processed: AtomicU64,
        checksum: AtomicU64,
    }

    impl WorkerStats {
        fn new() -> Self {
            Self {
                received: AtomicU64::new(0),
                dropped: AtomicU64::new(0),
                processed: AtomicU64::new(0),
                checksum: AtomicU64::new(0),
            }
        }
    }

    struct Config {
        bind: Ipv4Addr,
        port: u16,
        workers: usize,
        first_cpu: usize,
    }

    impl Config {
        fn parse() -> Self {
            let mut c = Self {
                bind: Ipv4Addr::UNSPECIFIED,
                port: DEFAULT_PORT,
                workers: DEFAULT_WORKERS,
                first_cpu: 0,
            };
            let mut a = env::args().skip(1);
            while let Some(flag) = a.next() {
                let value = a
                    .next()
                    .unwrap_or_else(|| panic!("missing value for {flag}"));
                match flag.as_str() {
                    "--port" => c.port = value.parse().expect("invalid --port"),
                    "--workers" => c.workers = value.parse().expect("invalid --workers"),
                    "--cpu" => c.first_cpu = value.parse().expect("invalid --cpu"),
                    "--bind" => c.bind = value.parse().expect("invalid --bind IPv4 address"),
                    _ => panic!("unknown argument: {flag}"),
                }
            }
            assert!(c.workers > 0, "--workers must be positive");
            c
        }
    }

    fn bind_reuseport(addr: SocketAddr) -> io::Result<OwnedFd> {
        unsafe {
            let fd = sys::socket(sys::AF_INET, sys::SOCK_DGRAM | sys::SOCK_CLOEXEC, 0);
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let one: core::ffi::c_int = 1;
            for opt in [sys::SO_REUSEADDR, sys::SO_REUSEPORT] {
                if sys::setsockopt(
                    fd,
                    sys::SOL_SOCKET,
                    opt,
                    (&one as *const core::ffi::c_int).cast(),
                    mem::size_of_val(&one) as _,
                ) < 0
                {
                    let error = io::Error::last_os_error();
                    let _ = sys::close(fd);
                    return Err(error);
                }
            }
            let rcvbuf: core::ffi::c_int = 64 * 1024 * 1024;
            let _ = sys::setsockopt(
                fd,
                sys::SOL_SOCKET,
                sys::SO_RCVBUF,
                (&rcvbuf as *const core::ffi::c_int).cast(),
                mem::size_of_val(&rcvbuf) as _,
            );
            let sockaddr = match addr {
                SocketAddr::V4(v4) => sys::sockaddr_in {
                    sin_family: sys::AF_INET as _,
                    sin_port: v4.port().to_be(),
                    sin_addr: sys::in_addr {
                        s_addr: u32::from_ne_bytes(v4.ip().octets()),
                    },
                    sin_zero: [0; 8],
                },
                _ => unreachable!(),
            };
            if sys::bind(
                fd,
                (&sockaddr as *const sys::sockaddr_in).cast(),
                mem::size_of_val(&sockaddr) as _,
            ) < 0
            {
                let error = io::Error::last_os_error();
                let _ = sys::close(fd);
                return Err(error);
            }
            Ok(OwnedFd::from_raw_fd(fd))
        }
    }

    fn pin_current_thread(cpu: usize) -> io::Result<()> {
        unsafe {
            let mut set: sys::cpu_set_t = mem::zeroed();
            if cpu >= set.bits.len() * 64 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "CPU index exceeds CPU_SETSIZE",
                ));
            }
            set.bits[cpu / 64] = 1_u64 << (cpu % 64);
            if sys::sched_setaffinity(0, mem::size_of::<sys::cpu_set_t>(), &set) != 0 {
                return Err(io::Error::last_os_error());
            }
        };
        Ok(())
    }

    #[inline(always)]
    fn monotonic_ns() -> u64 {
        unsafe {
            let mut ts: sys::timespec = mem::zeroed();
            sys::clock_gettime(sys::CLOCK_MONOTONIC, &mut ts);
            (ts.tv_sec as u64) * 1_000_000_000 + ts.tv_nsec as u64
        }
    }

    struct Batch {
        buffers: Box<[[u8; MAX_DATAGRAM]; BATCH_SIZE]>,
        messages: Box<[sys::mmsghdr; BATCH_SIZE]>,
        #[allow(dead_code)] // keeps iovec storage alive for msg_hdr.msg_iov pointers
        iovecs: Box<[sys::iovec; BATCH_SIZE]>,
    }

    impl Batch {
        fn new() -> Self {
            let buffers = Box::new([[0; MAX_DATAGRAM]; BATCH_SIZE]);
            let mut iovecs: Box<[sys::iovec; BATCH_SIZE]> = Box::new(
                [sys::iovec {
                    iov_base: core::ptr::null_mut(),
                    iov_len: 0,
                }; BATCH_SIZE],
            );
            let mut messages: Box<[sys::mmsghdr; BATCH_SIZE]> =
                Box::new(unsafe { MaybeUninit::zeroed().assume_init() });

            for index in 0..BATCH_SIZE {
                iovecs[index] = sys::iovec {
                    iov_base: buffers[index].as_ptr() as *mut _,
                    iov_len: MAX_DATAGRAM,
                };
                messages[index].msg_hdr.msg_iov = &mut iovecs[index];
                messages[index].msg_hdr.msg_iovlen = 1;
            }

            Self {
                buffers,
                messages,
                iovecs,
            }
        }
    }

    fn receiver(
        fd: OwnedFd,
        cpu: usize,
        ring: Arc<SpscRing<PacketMeta, RING_CAPACITY>>,
        stats: Arc<WorkerStats>,
        running: Arc<AtomicBool>,
    ) {
        pin_current_thread(cpu)
            .unwrap_or_else(|e| eprintln!("receiver CPU {cpu}: affinity failed: {e}"));
        let mut batch = Batch::new();

        while running.load(Ordering::Relaxed) {
            let count = unsafe {
                sys::recvmmsg(
                    fd.as_raw_fd(),
                    batch.messages.as_mut_ptr(),
                    BATCH_SIZE as u32,
                    0,
                    core::ptr::null_mut(),
                )
            };

            if count < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                eprintln!("recvmmsg: {error}");
                break;
            }

            let now = monotonic_ns();
            for index in 0..count as usize {
                let len = batch.messages[index].msg_len as usize;
                let meta = PacketMeta {
                    received_ns: now,
                    len: len as u16,
                    checksum: packet_checksum(&batch.buffers[index][..len]),
                };
                if ring.push(meta).is_err() {
                    stats.dropped.fetch_add(1, Ordering::Relaxed);
                } else {
                    stats.received.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    struct Histogram {
        buckets: [u64; 64],
        max: u64,
        samples: u64,
    }

    impl Histogram {
        fn new() -> Self {
            Self {
                buckets: [0; 64],
                max: 0,
                samples: 0,
            }
        }

        fn record(&mut self, ns: u64) {
            self.buckets[(63 - ns.leading_zeros()) as usize] += 1;
            self.max = self.max.max(ns);
            self.samples += 1;
        }

        fn percentile(&self, p: f64) -> u64 {
            if self.samples == 0 {
                return 0;
            }
            let wanted = ((self.samples as f64 * p).ceil() as u64).max(1);
            let mut total = 0;
            for (i, n) in self.buckets.iter().enumerate() {
                total += n;
                if total >= wanted {
                    return 1_u64 << i;
                }
            }
            self.max
        }
    }

    fn consumer(
        cpu: usize,
        ring: Arc<SpscRing<PacketMeta, RING_CAPACITY>>,
        stats: Arc<WorkerStats>,
        running: Arc<AtomicBool>,
    ) {
        pin_current_thread(cpu)
            .unwrap_or_else(|e| eprintln!("consumer CPU {cpu}: affinity failed: {e}"));
        let mut histogram = Histogram::new();
        let mut last = Instant::now();
        let mut last_count = 0;

        while running.load(Ordering::Relaxed) {
            if let Some(packet) = ring.pop() {
                histogram.record(monotonic_ns().saturating_sub(packet.received_ns));
                stats.processed.fetch_add(1, Ordering::Relaxed);
                stats
                    .checksum
                    .fetch_add(packet.checksum as u64, Ordering::Relaxed);
            } else {
                core::hint::spin_loop();
            }

            if last.elapsed() >= Duration::from_secs(1) {
                let count = stats.processed.load(Ordering::Relaxed);
                println!(
                    "pps={} p50~{}us p90~{}us p99~{}us p99.9~{}us max={}us drops={}",
                    count - last_count,
                    histogram.percentile(0.50) / 1000,
                    histogram.percentile(0.90) / 1000,
                    histogram.percentile(0.99) / 1000,
                    histogram.percentile(0.999) / 1000,
                    histogram.max / 1000,
                    stats.dropped.load(Ordering::Relaxed)
                );
                last_count = count;
                last = Instant::now();
            }
        }
    }

    pub fn run() -> io::Result<()> {
        let config = Config::parse();
        let addr = SocketAddr::new(IpAddr::V4(config.bind), config.port);
        let running = Arc::new(AtomicBool::new(true));

        println!(
            "udp_engine listening on {addr}, {} receiver/consumer pairs",
            config.workers
        );

        let mut joins = Vec::with_capacity(config.workers * 2);
        for worker in 0..config.workers {
            let ring = Arc::new(SpscRing::new());
            let stats = Arc::new(WorkerStats::new());
            let fd = bind_reuseport(addr)?;
            let receiver_cpu = config.first_cpu + worker * 2;
            let consumer_cpu = receiver_cpu + 1;

            let run = Arc::clone(&running);
            let r = Arc::clone(&ring);
            let s = Arc::clone(&stats);
            joins.push(thread::spawn(move || receiver(fd, receiver_cpu, r, s, run)));

            let run = Arc::clone(&running);
            joins.push(thread::spawn(move || {
                consumer(consumer_cpu, ring, stats, run)
            }));
        }

        for join in joins {
            let _ = join.join();
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() {
    linux::run().unwrap_or_else(|e| {
        eprintln!("udp_engine: {e}");
        std::process::exit(1);
    });
}
