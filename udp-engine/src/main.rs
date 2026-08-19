use udp_engine::batch::Packet;
use udp_engine::config::Config;
use udp_engine::receiver::Receiver;
use udp_engine::ring_buffer::spsc;
use udp_engine::worker::Worker;

const RING_CAPACITY: usize = 4096;

fn main() {
    let config = Config::new();
    let addr = config.address();

    let core_ids = core_affinity::get_core_ids().expect("enumerate cpu cores");

    // Half the cores receive, half process -- each receiver/worker pair
    // gets one core each, pinned. socket lives only on the receiver side (therefore loadbalancin works correct)
    let n_pairs = (core_ids.len() / 2).max(1);
    let mut rx_cores = Vec::new();
    let mut worker_cores = Vec::new();

    if core_ids.len() >= 2 {
        for i in 0..n_pairs {
            rx_cores.push(core_ids[i * 2]);
            worker_cores.push(core_ids[i * 2 + 1]);
        }
    } else {
        rx_cores.push(core_ids[0]);
        worker_cores.push(core_ids[0]);
    }

    eprintln!(
        "binding {addr}: {n_pairs} receiver/worker pair(s), {} core(s) available",
        core_ids.len()
    );

    let mut handles = Vec::with_capacity(n_pairs * 2);

    for i in 0..n_pairs {
        let (to_worker_tx, to_worker_rx) = spsc::<Packet>(RING_CAPACITY);
        let (to_receiver_tx, to_receiver_rx) = spsc::<Packet>(RING_CAPACITY);

        let receiver = Receiver::new(
            addr,
            rx_cores[i % rx_cores.len()],
            to_worker_tx,
            to_receiver_rx,
        )
        .unwrap_or_else(|e| panic!("create receiver socket #{i}: {e}"));
        handles.push(std::thread::spawn(move || receiver.run()));

        let worker = Worker::new(
            worker_cores[i % worker_cores.len()],
            to_worker_rx,
            to_receiver_tx,
        );
        handles.push(std::thread::spawn(move || worker.run()));
    }

    for h in handles {
        let _ = h.join();
    }
}
