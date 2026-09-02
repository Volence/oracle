//! A frame-time series that keeps every sample, so the report can quote a **median and a worst case**
//! rather than a mean. A mean hides a stutter: 3599 frames at 4 ms and one at 400 ms is a 4.1 ms mean and
//! an unusable player.
//!
//! Sixty seconds at 60 Hz is 3600 `f64`s per bucket — keeping them all costs ~29 KB per bucket and removes
//! every question about which estimator was used.

#[derive(Default)]
pub struct Series {
    samples: Vec<f64>,
}

impl Series {
    pub fn push(&mut self, ms: f64) {
        self.samples.push(ms);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Sorted copy — the report calls this once per bucket at the end, never on the hot path.
    fn sorted(&self) -> Vec<f64> {
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    fn quantile(sorted: &[f64], q: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        // Nearest-rank. With thousands of samples the interpolation choice is noise, and nearest-rank has
        // the property that every value reported is a value that actually happened.
        let idx = ((q * sorted.len() as f64).ceil() as usize).saturating_sub(1);
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    /// One fixed-width report line: mean, median, p95, p99, max, n.
    pub fn row(&self, name: &str) -> String {
        let s = self.sorted();
        format!(
            "{:<14} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8}",
            name,
            self.mean(),
            Self::quantile(&s, 0.50),
            Self::quantile(&s, 0.95),
            Self::quantile(&s, 0.99),
            s.last().copied().unwrap_or(0.0),
            self.len(),
        )
    }
}
