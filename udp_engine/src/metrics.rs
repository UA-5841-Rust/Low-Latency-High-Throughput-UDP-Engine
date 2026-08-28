use hdrhistogram::Histogram;
use std::time::{Duration, Instant};

pub struct Metrics {
    hist: Histogram<u64>,
    total_packets: u64,
    last_report_packets: u64,
    last_report_time: Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Metrics {
            hist: Histogram::<u64>::new(3).unwrap(),
            total_packets: 0,
            last_report_packets: 0,
            last_report_time: Instant::now(),
        }
    }

    pub fn record(&mut self, latency: Duration) {
        let elapsed = latency.as_nanos() as u64;
        let _ = self.hist.record(elapsed);
        self.total_packets += 1;
    }

    pub fn packet_count(&self) -> u64 {
        self.total_packets
    }

    pub fn print_report(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_report_time).as_secs_f64();
        let packets_since = self.total_packets - self.last_report_packets;
        let pps = if elapsed > 0.0 {
            packets_since as f64 / elapsed
        } else {
            0.0
        };

        println!("=== Performance Report ===");
        println!("Throughput: {:.0} packets/sec", pps);
        println!("Total packets processed: {}", self.total_packets);
        println!("Latency (microseconds):");
        println!("  p50:   {:.3}", self.hist.value_at_quantile(0.50) as f64 / 1000.0);
        println!("  p90:   {:.3}", self.hist.value_at_quantile(0.90) as f64 / 1000.0);
        println!("  p99:   {:.3}", self.hist.value_at_quantile(0.99) as f64 / 1000.0);
        println!("  p99.9: {:.3}", self.hist.value_at_quantile(0.999) as f64 / 1000.0);
        println!("  Max:   {:.3}", self.hist.max() as f64 / 1000.0);
        println!("==========================");

        self.last_report_time = now;
        self.last_report_packets = self.total_packets;
    }
}