use crate::{net, ring_buffer::SpscRingBuffer, worker};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;

pub struct UdpEngine {
    bind_addr: SocketAddr,
}

impl UdpEngine {
    pub fn new(port: u16) -> Self {
        let bind_addr = format!("0.0.0.0:{}", port)
            .parse()
            .expect("Invalid IP or Port");
        Self { bind_addr }
    }

    pub fn run(&self) {
        let core_ids = core_affinity::get_core_ids().expect("Failed to get CPU core");
        println!("Starting engine on {}", self.bind_addr);
        println!("Cores available: {}", core_ids.len());

        let mut handles = vec![];

        if core_ids.len() < 2 {
            panic!("Lock-free required at least 2 CPU cores");
        }

        let rx_core = core_ids[0];
        let tx_core = core_ids[1];

        let ring_buffer = Arc::new(SpscRingBuffer::<worker::Packet, 8192>::new());

        let consumer_buffer = Arc::clone(&ring_buffer);
        let consumer_handle = thread::spawn(move || {
            core_affinity::set_for_current(tx_core);
            println!("Consumer attached to core {}", tx_core.id);

            let mut processed = 0u64;
            loop {
                if let Some(_packet) = consumer_buffer.pop() {
                    processed += 1;
                    if processed.is_multiple_of(5_000_000) {
                        println!(
                            "Consumer processed {} millions packets",
                            processed / 1_000_000
                        );
                    }
                }
            }
        });
        handles.push(consumer_handle);

        let bind_addr = self.bind_addr;
        let socket =
            net::bind_reuseport_socket(bind_addr).expect("Failed to create SO_REUSEPORT socket");

        println!("Receiver attached to core {}", rx_core.id);
        let producer_handle = worker::start_worker(0, rx_core, socket, ring_buffer);
        handles.push(producer_handle);

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
