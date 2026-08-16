#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("udp_loadgen must run on Linux for this benchmark setup.");
}

#[cfg(target_os = "linux")]
fn main() {
    use std::env;
    use std::io;
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut host = Ipv4Addr::LOCALHOST;
    let mut port = 9000_u16;
    let mut threads = 1_usize;
    let mut size = 64_usize;
    let mut seconds = 20_u64;
    let mut args = env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .unwrap_or_else(|| panic!("missing value for {flag}"));
        match flag.as_str() {
            "--host" => host = value.parse().expect("invalid --host"),
            "--port" => port = value.parse().expect("invalid --port"),
            "--threads" => threads = value.parse().expect("invalid --threads"),
            "--size" => size = value.parse().expect("invalid --size"),
            "--seconds" => seconds = value.parse().expect("invalid --seconds"),
            _ => panic!("unknown argument: {flag}"),
        }
    }
    assert!((1..=2048).contains(&size), "--size must be 1..=2048");
    let running = Arc::new(AtomicBool::new(true));
    let sent = Arc::new(AtomicU64::new(0));
    let destination = SocketAddrV4::new(host, port);
    let mut joins = Vec::with_capacity(threads);
    for index in 0..threads {
        let running = Arc::clone(&running);
        let sent = Arc::clone(&sent);
        joins.push(thread::spawn(move || -> io::Result<()> {
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            let mut payload = [0_u8; 2048];
            payload[..8].copy_from_slice(&(index as u64).to_le_bytes());
            while running.load(Ordering::Relaxed) {
                socket.send_to(&payload[..size], destination)?;
                sent.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        }));
    }
    let start = Instant::now();
    let mut previous = 0;
    while start.elapsed() < Duration::from_secs(seconds) {
        thread::sleep(Duration::from_secs(1));
        let total = sent.load(Ordering::Relaxed);
        println!("sent_pps={}", total - previous);
        previous = total;
    }
    running.store(false, Ordering::Relaxed);
    for join in joins {
        join.join()
            .expect("generator thread panicked")
            .expect("send failed");
    }
    println!("total_packets={}", sent.load(Ordering::Relaxed));
}
