use crate::worker;
use std::net::UdpSocket;
use std::sync::Arc; 

pub struct UdpServer {
    port: u16,
}

impl UdpServer {
    pub fn new(port: u16) -> Self {
        Self {port}
    }   

    pub fn run(&self) {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        let socket = UdpSocket::bind(&bind_addr)
            .unwrap_or_else(|e| panic!("Failed to bind socket on {}: {}", bind_addr, e));

        let shared_socket = Arc::new(socket);

        let core_ids = core_affinity::get_core_ids().expect("Failed to get list of available cores of CPU");

        println!("Starting server on {}. Available cores: {}", bind_addr, core_ids.len());
        let mut worker_handles = Vec::with_capacity(core_ids.len());
        for (id, core_id) in core_ids.into_iter().enumerate() {
            let socket_clone = Arc::clone(&shared_socket);
            let handle = worker::start_worker(id, core_id, socket_clone);
            worker_handles.push(handle);
        }

        for handle in worker_handles {
            if let Err(e) = handle.join() {
                eprintln!("Failed terminating process: {:?}", e);
            }
        }
    }
}