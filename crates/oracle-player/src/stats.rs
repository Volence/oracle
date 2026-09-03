//! A frame-time series that keeps **every** sample, so the report can quote a median and a worst case
//! rather than a mean. A mean hides a stutter: 3599 frames at 4 ms and one at 400 ms is a 4.1 ms mean and
//! an unusable player.
//!
//! Sixty seconds at 60 Hz is 3600 `f64`s per bucket — ~29 KB, and it removes every question about which
//! estimator was used. Carried over from `crates/oracle-panels-spike/src/stats.rs` so the two sets of
//! numbers are computed the same way and can be compared directly.

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

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Sorted copy — called once per bucket at the end of a run, never on the hot path.
    fn sorted(&self) -> Vec<f64> {
        let mut v = self.samples.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Nearest-rank. With thousands of samples the interpolation choice is noise, and nearest-rank has the
    /// property that every value reported is a value that actually happened.
    fn quantile(sorted: &[f64], q: f64) -> f64 {
        if sorted.is_empty() {
            return 0.0;
        }
        let idx = ((q * sorted.len() as f64).ceil() as usize).saturating_sub(1);
        sorted[idx.min(sorted.len() - 1)]
    }

    pub fn mean(&self) -> f64 {
        if self.samples.is_empty() {
            return 0.0;
        }
        self.samples.iter().sum::<f64>() / self.samples.len() as f64
    }

    pub fn median(&self) -> f64 {
        Self::quantile(&self.sorted(), 0.50)
    }

    pub fn max(&self) -> f64 {
        self.sorted().last().copied().unwrap_or(0.0)
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

    pub fn header() -> String {
        format!(
            "{:<14} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
            "part", "mean", "median", "p95", "p99", "max", "n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason this type exists, stated as a test: one 400 ms stutter is invisible in the mean and
    /// unmissable in the max.
    #[test]
    fn a_stutter_survives_the_median_but_shows_in_the_max() {
        let mut s = Series::default();
        for _ in 0..3599 {
            s.push(4.0);
        }
        s.push(400.0);
        assert!((s.mean() - 4.11).abs() < 0.01, "mean = {}", s.mean());
        assert_eq!(s.median(), 4.0);
        assert_eq!(s.max(), 400.0);
    }

    /// Nearest-rank reports a value that actually happened, at both ends.
    #[test]
    fn quantiles_are_observed_values() {
        let mut s = Series::default();
        for v in [5.0, 1.0, 3.0, 2.0, 4.0] {
            s.push(v);
        }
        assert_eq!(s.median(), 3.0);
        assert_eq!(s.max(), 5.0);
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn an_empty_series_reports_zeroes_rather_than_panicking() {
        let s = Series::default();
        assert!(s.is_empty());
        assert_eq!(s.mean(), 0.0);
        assert_eq!(s.median(), 0.0);
        assert_eq!(s.max(), 0.0);
    }
}
