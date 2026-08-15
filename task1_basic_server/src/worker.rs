use core_affinity::CoreId;
use std::net::UdpSocket;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub fn start_worker(
    worker_id: usize,
    core_id: CoreId,
    socket: Arc<UdpSocket>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        if core_affinity::set_for_current(core_id) {
            println!("Worker {} successfully assigned to the core {}", worker_id, core_id.id);
        } else {
            eprintln!("Error: failed to assign Worker {} to the core {}", worker_id, core_id.id);
        }

        let mut buffer = [0u8; 1024];
        let mut packet_count = 0u64;

        loop {
            match socket.recv_from(&mut buffer) {
                Ok((_bytes_read, _src_addr)) => {
                    packet_count += 1;

                    if packet_count % 5_000_000 == 0 {
                        println!("Worker {} prossed {} millions of packets", worker_id, packet_count / 1_000_000);
                    }
                }
                Err(e) => {
                    eprintln!("Worker {}: error reading packet - {}", worker_id, e);
                }
            }
        }
    })
}
