use std::time::Instant;

/// Periodic stream statistics tracker.
///
/// Accumulates frame count and byte totals, then computes FPS/bitrate/avg
/// frame size when the reporting interval (1 second) has elapsed.
pub struct StreamStats {
    start: Instant,
    frames: u64,
    bytes: u64,
    total_frames: u64,
}

impl StreamStats {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            frames: 0,
            bytes: 0,
            total_frames: 0,
        }
    }

    /// Record a completed frame.
    pub fn record_frame(&mut self, size: u64) {
        self.frames += 1;
        self.bytes += size;
        self.total_frames += 1;
    }

    /// Total frames since creation.
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// If the reporting interval has elapsed, return a formatted stats line
    /// and reset the interval counters. `fps_limit` is shown when `Some`.
    pub fn report_if_due(&mut self, prefix: &str, fps_limit: Option<u32>) -> Option<String> {
        let elapsed = self.start.elapsed();
        if elapsed.as_secs() < 1 {
            return None;
        }

        let fps = self.frames as f64 / elapsed.as_secs_f64();
        let mbps = (self.bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
        let avg_kb = if self.frames > 0 { self.bytes / self.frames / 1024 } else { 0 };

        let limit_str = match fps_limit {
            Some(l) => format!(" (limit: {})", l),
            None => String::new(),
        };

        let report = format!(
            "[{}] FPS: {:.1}{} | Bitrate: {:.2} Mbps | Avg: {}KB | Total: {} frames",
            prefix, fps, limit_str, mbps, avg_kb, self.total_frames
        );

        self.reset();
        Some(report)
    }

    /// Reset interval counters.
    pub fn reset(&mut self) {
        self.start = Instant::now();
        self.frames = 0;
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_frames_accumulates() {
        let mut stats = StreamStats::new();
        assert_eq!(stats.total_frames(), 0);
        stats.record_frame(1000);
        stats.record_frame(2000);
        stats.record_frame(3000);
        assert_eq!(stats.total_frames(), 3);
    }

    #[test]
    fn report_not_due_before_one_second() {
        let mut stats = StreamStats::new();
        stats.record_frame(1000);
        // Immediately after creation, less than 1 second has elapsed
        assert!(stats.report_if_due("TEST", None).is_none());
    }

    #[test]
    fn reset_clears_interval_counters() {
        let mut stats = StreamStats::new();
        stats.record_frame(5000);
        stats.record_frame(5000);
        stats.reset();
        // Total frames persists across resets
        assert_eq!(stats.total_frames(), 2);
    }

    #[test]
    fn zero_frames_no_division_error() {
        let mut stats = StreamStats::new();
        // Force elapsed > 1s by resetting start to past
        stats.start = Instant::now() - std::time::Duration::from_secs(2);
        // No frames recorded — should not panic on division
        let report = stats.report_if_due("TEST", Some(60));
        assert!(report.is_some());
        assert!(report.unwrap().contains("Avg: 0KB"));
    }
}
