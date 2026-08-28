// use core_affinity::CoreId;
use std::thread;
use std::time::Instant;

mod server;
mod worker;
mod metrics;

fn main() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    if core_ids.len() < 2 {
        panic!("Not enough CPU cores");
    }

    let rx_core = core_ids[0];
    let worker_core = core_ids[1];
    let port = 8080;

    let queue: &'static worker::SpscQueue = Box::leak(worker::SpscQueue::new());

    let rx_handle = thread::spawn(move || {
        let success = core_affinity::set_for_current(rx_core);
        if !success {
            panic!("RX pin failed");
        }

        let fd = server::create_reuseport_socket(port);
        let mut ctx = server::ReceiveContext::new();
        ctx.prepare();

        loop {
            let count = server::receive_batch(fd, &mut ctx);
            for i in 0..count {
                while !queue.push(&ctx.packets[i]) {
                    std::hint::spin_loop();
                }
            }
        }
    });

    let _worker_handle = thread::spawn(move || {
        let success = core_affinity::set_for_current(worker_core);
        if !success {
            panic!("Worker pin failed");
        }

        let mut processed: u64 = 0;
        let mut tracker = metrics::Metrics::new();

        loop {
            if let Some(_packet) = queue.pop() {
                let start = Instant::now();

                tracker.record(start);
                processed += 1;

                if processed % 1_000_000 == 0 {
                    tracker.print_report();
                }
            } else {
                std::hint::spin_loop();
            }
        }
    });

    rx_handle.join().unwrap();
    // worker_handle.join().unwrap();
}