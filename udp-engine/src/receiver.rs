use crate::batch::{self, BATCH_SIZE, BatchState, Packet};
use crate::metrics::ThreadMetrics;
use crate::ring_buffer::{Consumer, Producer};
use crate::socket::create_reuseport_socket;
use core_affinity::CoreId;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};
use std::time::Instant;

/// Owns one `SO_REUSEPORT` socket and does *all* the I/O for one pipeline
/// lane: batched receive, hand raw packets to the paired worker, batched
/// send of whatever the worker has finished with.
///
/// Workers never touch a socket. That means receiver has to do both halves
/// of the I/O (recv *and* send) on its own socket, which is also the only
/// correct way to do it: the source address a client sees on a reply is
/// whichever socket sent it, and only the receiver's socket is bound to
/// the server's address.
pub struct Receiver {
    _socket: socket2::Socket, // keeps the fd alive for as long as Receiver exists
    fd: RawFd,
    batch: Box<BatchState>,
    to_worker: Producer<Packet>,
    from_worker: Consumer<Packet>,
    core: CoreId,
    packets_received: u64,
    packets_sent: u64,
    packets_dropped: u64,      // to_worker ring was full
    packets_send_dropped: u64, // sendmmsg couldn't send (socket buffer full)
    metrics: ThreadMetrics,
}

impl Receiver {
    pub fn new(
        addr: SocketAddr,
        core: CoreId,
        to_worker: Producer<Packet>,
        from_worker: Consumer<Packet>,
    ) -> std::io::Result<Self> {
        let socket = create_reuseport_socket(addr)?;
        let fd = socket.as_raw_fd();
        Ok(Self {
            _socket: socket,
            fd,
            batch: BatchState::new(),
            to_worker,
            from_worker,
            core,
            packets_received: 0,
            packets_sent: 0,
            packets_dropped: 0,
            packets_send_dropped: 0,
            metrics: ThreadMetrics::new(),
        })
    }

    /// Pins the calling thread to `self.core` and runs the receive/send
    /// loop forever. Call this as the body of a spawned thread.
    pub fn run(mut self) {
        if !core_affinity::set_for_current(self.core) {
            panic!("failed to pin receiver thread to core #{}", self.core.id);
        }

        let mut last_report = Instant::now();

        // recv_batch never blocks (MSG_DONTWAIT), so this loop always comes back around to
        // check the outbound ring every iteration. When there's truly
        // nothing to do on either side, spin briefly for latency, then
        // yield so a non-isolated shared core doesn't get starved.
        // const SPIN_BEFORE_YIELD: u32 = 10_000;
        // let mut idle_spins: u32 = 0;

        // Instant::now() is a cheap vDSO call (~20-30ns, no real syscall),
        let mut loop_counter: u32 = 0;
        const TIME_CHECK_MASK: u32 = 1023; // check every 1024 iterations

        loop {
            loop_counter = loop_counter.wrapping_add(1);
            let mut did_something = false;

            // 1. Drain whatever's queued right now (0..BATCH_SIZE, never
            //    blocks) and hand each packet to the worker.
            match self.batch.recv_batch(self.fd) {
                Ok(n) => {
                    if n > 0 {
                        did_something = true;
                        self.packets_received += n as u64;
                        for i in 0..n {
                            if self.to_worker.push(self.batch.packets[i]).is_err() {
                                // Worker can't keep up -- ring is full.
                                // Drop rather than block: blocking here
                                // would stall this whole lane, worse than
                                // losing one packet.
                                self.packets_dropped += 1;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[rx core {}] recvmmsg error: {e}", self.core.id);
                }
            }

            // 2. Drain whatever the worker has finished with and send it,
            //    batched. Bounded by BATCH_SIZE per pass so a fast worker
            //    can't make step 1 starve indefinitely.
            let mut n_out = 0;
            while n_out < BATCH_SIZE {
                match self.from_worker.pop() {
                    Some(packet) => {
                        self.batch.packets[n_out] = packet;
                        n_out += 1;
                    }
                    None => break,
                }
            }
            if n_out > 0 {
                did_something = true;
                match self.batch.send_batch(self.fd, n_out) {
                    Ok(sent) => {
                        self.packets_sent += sent as u64;
                        // send_batch never blocks -- a short count here
                        // means the socket's send buffer was full, not an
                        // error.
                        self.packets_send_dropped += (n_out - sent) as u64;

                        let now_ns = batch::monotonic_now_ns();
                        for i in 0..sent {
                            let latency = now_ns.saturating_sub(self.batch.packets[i].recv_ts_ns);
                            self.metrics.record(latency);
                        }

                        if self.metrics.is_batch_full() {
                            self.metrics.report(self.core.id);
                        }
                    }
                    Err(e) => eprintln!("[rx core {}] sendmmsg error: {e}", self.core.id),
                }
            }

            if did_something {
                // idle_spins = 0;
            } else {
                // idle_spins += 1;
                // if idle_spins < SPIN_BEFORE_YIELD {
                //     std::hint::spin_loop();
                // } else {
                //     std::thread::yield_now();
                // }
                std::hint::spin_loop();
            }

            // Heartbeat, not a hot-path log -- checked only every 1024
            // iterations, and even then only prints at most once/sec.
            if loop_counter & TIME_CHECK_MASK == 0 && last_report.elapsed().as_secs() >= 1 {
                eprintln!(
                    "[rx core {}] recv={} sent={} dropped(ring full)={} dropped(send full)={}",
                    self.core.id,
                    self.packets_received,
                    self.packets_sent,
                    self.packets_dropped,
                    self.packets_send_dropped,
                );
                self.metrics.report(self.core.id);
                last_report = Instant::now();
            }
        }
    }
}
