use crate::net;
use std::net::SocketAddr;
use std::thread;

pub struct UdpEngine {
    bind_addr: SocketAddr,
}

impl UdpEngine {
    pub fn new(port: u16) -> Self {
        let bind_addr = format!("0.0.0.0:{}", port).parse().expect("Invalid IP or Port");
        Self { bind_addr }
    }

    pub fn run(&self) {
        let core_ids = core_affinity::get_core_ids().expect("Failed to get CPU core");
        println!("Starting engine on {}", self.bind_addr);
        println!("Cores available: {}", core_ids.len());

        let mut handles = vec![];

        for (id, core_id) in core_ids.into_iter().enumerate() {
            let bind_addr = self.bind_addr;

            let handle = thread::spawn(move || {
                core_affinity::set_for_current(core_id);

                let _socket = net::bind_reuseport_socket(bind_addr).expect("Failed to create SO_REUSEPORT socket");

                println!("Worker {} attached to core {} by it's own socket", id, core_id.id);

                thread::park();
            });
            handles.push(handle);
        }

        for handle in handles {
             handle.join().unwrap();
        }
    }
}
