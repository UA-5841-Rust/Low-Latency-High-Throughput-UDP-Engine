use hdrhistogram::Histogram;

pub struct ThreadMetrics {
    histogram: Histogram<u64>,
    packets_processed: u64,
}

impl ThreadMetrics {
    pub fn new() -> Self {
        Self {
            histogram: Histogram::new_with_bounds(1, 1_000_000_000, 3)
                .expect("failed to create hdr histogram"),
            packets_processed: 0,
        }
    }

    #[inline]
    pub fn record(&mut self, latency_ns: u64) {
        let _ = self.histogram.record(latency_ns);
        self.packets_processed += 1;
    }

    #[inline]
    pub fn is_batch_full(&self) -> bool {
        self.packets_processed >= 1_000_000
    }

    pub fn report(&mut self, core_id: usize) {
        if self.packets_processed == 0 {
            return;
        }

        eprintln!(
            "[rx core {}] {} pkts | Latency: p50={:.2}µs p99={:.2}µs p99.9={:.2}µs max={:.2}µs",
            core_id,
            self.packets_processed,
            self.histogram.value_at_quantile(0.50) as f64 / 1000.0,
            self.histogram.value_at_quantile(0.99) as f64 / 1000.0,
            self.histogram.value_at_quantile(0.999) as f64 / 1000.0,
            self.histogram.max() as f64 / 1000.0,
        );

        self.histogram.reset();
        self.packets_processed = 0;
    }
}
