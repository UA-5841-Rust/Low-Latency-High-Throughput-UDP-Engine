use core_affinity::CoreId;
use std::net::UdpSocket;
use std::os::unix::io::AsRawFd;
use std::ptr;
use std::thread::{self, JoinHandle};

const BATCH_SIZE: usize = 32;
const BUF_SIZE: usize = 1024;

// Align memory to cache line boundaries to prevent false sharing
#[repr(align(64))]
struct AlignedBuffer([u8; BUF_SIZE]);

pub fn start_worker(
    worker_id: usize,
    core_id: CoreId,
    socket: UdpSocket,
) -> JoinHandle<()> {
    thread::spawn(move || {
        // Pin thread to a specific CPU core
        core_affinity::set_for_current(core_id);

        let fd = socket.as_raw_fd();

        // Pre-allocate buffers for the zero-allocation hot path
        let mut buffers: Vec<AlignedBuffer> = (0..BATCH_SIZE)
            .map(|_| AlignedBuffer([0; BUF_SIZE]))
            .collect();

        // Initialize C structures for libc
        let mut iovecs: [libc::iovec; BATCH_SIZE] = unsafe {std::mem::zeroed()};
        let mut msgs: [libc::mmsghdr; BATCH_SIZE] = unsafe {std::mem::zeroed()};

        // Map C structures to the pre-allocated Rust buffers
        for i in 0..BATCH_SIZE {
            iovecs[i].iov_base = buffers[i].0.as_mut_ptr() as *mut libc::c_void;
            iovecs[i].iov_len = BUF_SIZE;

            msgs[i].msg_hdr.msg_iov = &mut iovecs[i];
            msgs[i].msg_hdr.msg_iovlen = 1;
        }

        let mut packet_count = 0u64;

        // Hot path: process incoming packets
        loop {
            // Batch read packets via system call
            let pkts_received = unsafe {
                libc::recvmmsg(
                    fd, 
                    msgs.as_mut_ptr(),
                    BATCH_SIZE as u32,
                    0,
                    ptr::null_mut(),
                )
            };

            if pkts_received > 0 {
                packet_count += pkts_received as u64;

                // Log performance metrics
                if packet_count % 5_000_000 < (pkts_received as u64) {
                    println!("Worker {} processed {} millions packets", worker_id, packet_count / 1_000_000);
                }
            } else if pkts_received < 0 {
                let err = std::io::Error::last_os_error();
                eprintln!("Worker {} error recvmmsg: {}", worker_id, err);
            }
        }
    })
}
