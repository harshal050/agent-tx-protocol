//! Latency summaries.

use std::time::Duration;

use crate::report::Summary;

pub fn micros(d: Duration) -> f64 {
    d.as_secs_f64() * 1_000_000.0
}

/// Nearest-rank percentile summary of microsecond samples.
pub fn summarize(samples: &mut [f64]) -> Summary {
    if samples.is_empty() {
        return Summary::default();
    }
    samples.sort_by(f64::total_cmp);
    let pct = |p: f64| {
        let rank = ((p / 100.0) * samples.len() as f64).ceil() as usize;
        samples[rank.clamp(1, samples.len()) - 1]
    };
    Summary {
        samples: samples.len(),
        mean_us: samples.iter().sum::<f64>() / samples.len() as f64,
        min_us: samples[0],
        p50_us: pct(50.0),
        p90_us: pct(90.0),
        p95_us: pct(95.0),
        p99_us: pct(99.0),
        max_us: samples[samples.len() - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let mut samples: Vec<f64> = (1..=100).map(f64::from).collect();
        samples.reverse();
        let s = summarize(&mut samples);
        assert_eq!(s.samples, 100);
        assert_eq!(s.p50_us, 50.0);
        assert_eq!(s.p99_us, 99.0);
        assert_eq!(s.min_us, 1.0);
        assert_eq!(s.max_us, 100.0);
        assert_eq!(s.mean_us, 50.5);
        assert_eq!(summarize(&mut []).samples, 0);
    }
}
