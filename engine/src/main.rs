use std::{
    ffi::c_void,
    net::{Ipv4Addr, SocketAddrV4},
    os::fd::AsRawFd,
    ptr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use hdrhistogram::Histogram;
use libc::{iovec, mmsghdr, recvmmsg};
use rtrb::{Consumer, Producer, RingBuffer};
use socket2::{Domain, Protocol, Socket, Type};

const BATCH_SIZE: usize = 64;
const BUF_SIZE: usize = 2048;
const QUEUE_CAPACITY: usize = 524_288;

#[derive(Copy, Clone)]
struct Packet {
    data: [u8; BUF_SIZE],
    len: usize,
    received_at: Instant,
}

#[repr(align(64))]
struct WorkerStats {
    packets: AtomicUsize,
    bytes: AtomicUsize,
}

fn main() {
    let addr = SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 8080);

    let core_ids = core_affinity::get_core_ids().expect("Can not get CPU cores");
    assert!(core_ids.len() >= 2, "Has to have at least 2 cores");

    let (producer, consumer) = RingBuffer::<Packet>::new(QUEUE_CAPACITY);

    let stats = Arc::new(WorkerStats {
        packets: AtomicUsize::new(0),
        bytes: AtomicUsize::new(0),
    });

    let rx_core = core_ids[0];
    let rx_producer = producer;
    let rx_thread = thread::spawn(move || {
        core_affinity::set_for_current(rx_core);
        run_receiver(addr, rx_producer);
    });

    let worker_core = core_ids[1];
    let worker_stats = Arc::clone(&stats);
    let worker_thread = thread::spawn(move || {
        core_affinity::set_for_current(worker_core);
        run_worker(consumer, worker_stats);
    });

    let mut last_packets = 0;
    loop {
        thread::sleep(Duration::from_secs(1));
        let current_packets = stats.packets.load(std::sync::atomic::Ordering::Relaxed);
        let pps = current_packets - last_packets;
        last_packets = current_packets;

        println!("PPS: {}", pps);
    }
}

fn setup_socket(addr: SocketAddrV4) -> Socket {
    let socket =
        Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).expect("Failed to get socket");

    socket
        .set_reuse_port(true)
        .expect("failed to set SO_REUSEPORT");
    socket.bind(&addr.into()).expect("failed to bind socket");

    socket
}

fn run_receiver(addr: SocketAddrV4, mut producer: Producer<Packet>) {
    let socket = setup_socket(addr);
    let fd = socket.as_raw_fd();

    let mut buffers = [[0u8; BUF_SIZE]; BATCH_SIZE];
    let mut iovecs = [iovec {
        iov_base: ptr::null_mut(),
        iov_len: 0,
    }; BATCH_SIZE];

    let mut msgs = [mmsghdr {
        msg_hdr: libc::msghdr {
            msg_name: ptr::null_mut(),
            msg_namelen: 0,
            msg_iov: ptr::null_mut(),
            msg_iovlen: 1,
            msg_control: ptr::null_mut(),
            msg_controllen: 0,
            msg_flags: 0,
        },
        msg_len: 0,
    }; BATCH_SIZE];

    for i in 0..BATCH_SIZE {
        iovecs[i].iov_base = buffers[i].as_mut_ptr() as *mut c_void;
        iovecs[i].iov_len = BUF_SIZE;
        msgs[i].msg_hdr.msg_iov = &mut iovecs[i];
    }
    loop {
        let packets_received =
            unsafe { recvmmsg(fd, msgs.as_mut_ptr(), BATCH_SIZE as u32, 0, ptr::null_mut()) };

        if packets_received > 0 {
            let now = Instant::now();
            for i in 0..(packets_received as usize) {
                let len = msgs[i].msg_len as usize;

                let mut pkt = Packet {
                    data: [0; BUF_SIZE],
                    len,
                    received_at: now,
                };

                unsafe {
                    ptr::copy_nonoverlapping(buffers[i].as_ptr(), pkt.data.as_mut_ptr(), len);
                }

                let _ = producer.push(pkt);
            }
        }
    }
}

fn run_worker(mut consumer: Consumer<Packet>, stats: Arc<WorkerStats>) {
    let mut hist = Histogram::<u64>::new_with_bounds(1, 1_000_000, 3).unwrap();
    let mut batch_count = 0;

    loop {
        if let Ok(packet) = consumer.pop() {
            let latency_us = packet.received_at.elapsed().as_micros() as u64;
            let _ = hist.record(latency_us);

            let payload = &packet.data[..packet.len];

            stats.packets.fetch_add(1, Ordering::Relaxed);
            stats.bytes.fetch_add(packet.len, Ordering::Relaxed);

            batch_count += 1;

            if batch_count >= 1_000_000 {
                println!(
                    "Worker Latency -> p50: {}us | p90: {}us | p99: {}us | p99.9: {}us | Max: {}us",
                    hist.value_at_percentile(50.0),
                    hist.value_at_percentile(90.0),
                    hist.value_at_percentile(99.0),
                    hist.value_at_percentile(99.9),
                    hist.max()
                );
                batch_count = 0;
                hist.reset();
            }
        }
    }
}
