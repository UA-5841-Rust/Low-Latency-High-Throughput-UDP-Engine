use crate::batch::Packet;
use crate::ring_buffer::{Consumer, Producer};
use core_affinity::CoreId;
use std::time::Instant;

pub struct Worker {
    from_receiver: Consumer<Packet>,
    to_receiver: Producer<Packet>,
    core: CoreId,
}

impl Worker {
    pub fn new(
        core: CoreId,
        from_receiver: Consumer<Packet>,
        to_receiver: Producer<Packet>,
    ) -> Self {
        Self {
            from_receiver,
            to_receiver,
            core,
        }
    }

    /// Pins the calling thread to `self.core` and runs the process loop
    /// forever. Call this as the body of a spawned thread.
    pub fn run(mut self) {
        if !core_affinity::set_for_current(self.core) {
            panic!("failed to pin worker thread to core #{}", self.core.id);
        }

        // const SPIN_BEFORE_YIELD: u32 = 10_000;
        // let mut empty_spins: u32 = 0;

        let mut loop_counter: u32 = 0;
        const TIME_CHECK_MASK: u32 = 1023;
        let mut last_report = Instant::now();
        let mut packets_processed: u64 = 0;
        let mut replies_dropped: u64 = 0; // to_receiver ring was full

        loop {
            loop_counter = loop_counter.wrapping_add(1);

            match self.from_receiver.pop() {
                Some(packet) => {
                    // empty_spins = 0;

                    let reply = self.process(packet);
                    // If the outbound ring is full the receiver is behind
                    // on sending -- drop rather than block. Blocking a
                    // worker here would stall this whole lane's
                    // processing. Counted (not just silently discarded) so
                    // it shows up in the report -- if this climbs, the
                    // receiver can't drain fast enough, which is a real
                    // capacity problem, not noise to ignore.
                    match self.to_receiver.push(reply) {
                        Ok(()) => packets_processed += 1,
                        Err(_) => replies_dropped += 1,
                    }
                }
                None => {
                    // empty_spins += 1;
                    // if empty_spins < SPIN_BEFORE_YIELD {
                    //     std::hint::spin_loop();
                    // } else {
                    //     std::thread::yield_now();
                    // }
                    std::hint::spin_loop();
                }
            }

            if loop_counter & TIME_CHECK_MASK == 0
                && last_report.elapsed().as_secs() >= 1
                && (packets_processed > 0 || replies_dropped > 0)
            {
                eprintln!(
                    "[wx core {}] processed={} dropped(ring full)={}",
                    self.core.id, packets_processed, replies_dropped
                );
                last_report = Instant::now();
            }
        }
    }

    /// Hook for real business logic (parsing, validation, checksums,
    /// whatever the task needs) -- right now this is a straight echo
    /// (sent back completely unchanged).
    fn process(&self, packet: Packet) -> Packet {
        packet
    }
}
