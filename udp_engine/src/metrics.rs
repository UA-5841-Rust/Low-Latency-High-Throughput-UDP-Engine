use hdrhistogram::Histogram;
use std::time::Instant;

pub struct Metrics {
    hist: Histogram<u64>,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            hist: Histogram::<u64>::new(3).unwrap(),
        }
    }

    pub fn record(&mut self, start: Instant) {
        let elapsed = start.elapsed().as_nanos() as u64;
        let _ = self.hist.record(elapsed);
    }

    pub fn print_report(&self) {
        println!("Latency Report (ns):");
        println!("p50:   {}", self.hist.value_at_quantile(0.50));
        println!("p90:   {}", self.hist.value_at_quantile(0.90));
        println!("p99:   {}", self.hist.value_at_quantile(0.99));
        println!("p99.9: {}", self.hist.value_at_quantile(0.999));
        println!("Max:   {}", self.hist.max());
    }
}