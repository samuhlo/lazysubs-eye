//! [NOTE] REGRESSION BUDGETS
//!
//! These ceilings intentionally allow CI variance. Measured baselines live in
//! `perf/baseline.json`; budgets catch regressions, not benchmark targets.

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PerformanceBudget {
    pub waybar_cached_ms: u64,
    pub first_render_ms: u64,
    pub incremental_scan_ms: u64,
    pub refresh_global_ms: u64,
    pub backfill_day_ms: u64,
}

impl Default for PerformanceBudget {
    fn default() -> Self {
        Self {
            waybar_cached_ms: 10,
            first_render_ms: 150,
            incremental_scan_ms: 500,
            refresh_global_ms: 8_000,
            backfill_day_ms: 500,
        }
    }
}

/// [NOTE] SINGLE-SAMPLE REGRESSION GUARD
///
/// Runs optional warmups outside timing, then measures one operation against a
/// 10% tolerance. WHY: warmups remove one-time setup noise while the tolerance
/// avoids failing shared CI for minor scheduler variance.
pub fn measure_budget(
    budget: Duration,
    warmups: usize,
    mut operation: impl FnMut(),
) -> Result<Duration, String> {
    for _ in 0..warmups {
        operation();
    }
    let start = Instant::now();
    operation();
    let elapsed = start.elapsed();
    let ceiling = budget.mul_f64(1.10);
    if elapsed <= ceiling {
        Ok(elapsed)
    } else {
        Err(format!(
            "presupuesto excedido: {}µs > {}µs (incluye 10% de tolerancia)",
            elapsed.as_micros(),
            ceiling.as_micros()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budgets_tienen_los_techos_documentados() {
        let budget = PerformanceBudget::default();
        assert_eq!(budget.waybar_cached_ms, 10);
        assert_eq!(budget.first_render_ms, 150);
        assert_eq!(budget.incremental_scan_ms, 500);
        assert_eq!(budget.refresh_global_ms, 8_000);
        assert_eq!(budget.backfill_day_ms, 500);
    }

    #[test]
    fn measure_budget_aplica_warmup_y_tolerancia() {
        let mut calls = 0;
        assert!(measure_budget(Duration::from_millis(100), 2, || calls += 1).is_ok());
        assert_eq!(calls, 3);
        assert!(measure_budget(Duration::ZERO, 0, || {
            std::thread::sleep(Duration::from_millis(1));
        })
        .is_err());
    }
}
