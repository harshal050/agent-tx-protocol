//! Error cleaner: context reduction and parse latency on realistic traces.

use std::hint::black_box;
use std::time::Instant;

use agenttx::parser::ErrorCleaner;

use crate::fixtures::ERROR_FIXTURES;
use crate::report::{CleanerResult, RunConfig};
use crate::stats::{micros, summarize};

pub fn run(config: &RunConfig) -> Vec<CleanerResult> {
    let cleaner = ErrorCleaner::global();
    let iterations = if config.quick { 2_000 } else { 20_000 };

    ERROR_FIXTURES
        .iter()
        .map(|fixture| {
            let hint = cleaner.clean(fixture.raw);
            for _ in 0..200 {
                black_box(cleaner.clean(black_box(fixture.raw)));
            }
            let mut samples = Vec::with_capacity(iterations);
            for _ in 0..iterations {
                let started = Instant::now();
                black_box(cleaner.clean(black_box(fixture.raw)));
                samples.push(micros(started.elapsed()));
            }
            let raw_bytes = fixture.raw.len();
            let hint_bytes = hint.text.len();
            CleanerResult {
                id: fixture.id,
                label: fixture.label,
                runtime: fixture.runtime,
                raw: fixture.raw,
                raw_bytes,
                raw_lines: fixture.raw.lines().count(),
                hint_bytes,
                reduction_pct: (1.0 - hint_bytes as f64 / raw_bytes as f64) * 100.0,
                hint: hint.text,
                rule: hint.rule,
                category: hint.category.as_str(),
                key: hint.key,
                parse: summarize(&mut samples),
            }
        })
        .collect()
}
