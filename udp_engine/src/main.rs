use std::thread;

mod server;
mod worker;
mod metrics;

fn main() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    if core_ids.len() < 2 {
        panic!("Need at least 2 CPU cores");
    }

    let port = 8080;
    let num_pairs = core_ids.len() / 2;

    println!(
        "Starting UDP engine with {} RX/Worker pairs on {} cores...",
        num_pairs,
        core_ids.len()
    );

    let mut handles = vec![];

    for pair_id in 0..num_pairs {
        let rx_core = core_ids[pair_id * 2];
        let worker_core = core_ids[pair_id * 2 + 1];

        let queue: &'static worker::SpscQueue = Box::leak(worker::SpscQueue::new());

        let rx_handle = thread::spawn(move || {
            if !core_affinity::set_for_current(rx_core) {
                panic!("RX core pin failed");
            }

            let fd = server::create_reuseport_socket(port);
            let mut ctx = server::ReceiveContext::new();
            ctx.prepare();

            let mut rx_count: u64 = 0;
            let mut last_report = std::time::Instant::now();

            loop {
                let count = server::receive_batch(fd, &mut ctx);
                for i in 0..count {
                    while !queue.push(&ctx.packets[i]) {
                        std::hint::spin_loop();
                    }
                }

                rx_count += count as u64;

                if rx_count > 0 && rx_count % 1_000_000 == 0 {
                    let elapsed = last_report.elapsed().as_secs_f64();
                    if elapsed > 0.0 {
                        println!(
                            "[RX #{}] Last 1M packets in {:.3}s -> {:.0} PPS",
                            pair_id,
                            elapsed,
                            1_000_000.0 / elapsed
                        );
                    }
                    last_report = std::time::Instant::now();
                }
            }
        });

        let worker_handle = thread::spawn(move || {
            if !core_affinity::set_for_current(worker_core) {
                panic!("Worker core pin failed");
            }

            let mut tracker = metrics::Metrics::new();

            loop {
                if let Some(packet) = queue.pop() {
                    let latency = packet.timestamp.elapsed();
                    tracker.record(latency);

                    if tracker.packet_count() % 1_000_000 == 0 && tracker.packet_count() > 0 {
                        tracker.print_report();
                    }
                } else {
                    std::thread::yield_now();
                }
            }
        });

        handles.push(rx_handle);
        handles.push(worker_handle);
    }

    for handle in handles {
        let _ = handle.join();
    }
}