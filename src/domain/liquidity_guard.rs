//! Liquidity Guard
//!
//! Real-time liquidity monitoring with emergency exit detection.
//! Triggers instant exits when liquidity drops significantly.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use thiserror::Error;

/// Default emergency exit threshold (30% drop)
pub const DEFAULT_EMERGENCY_EXIT_THRESHOLD_PCT: f64 = 30.0;

/// Default minimum liquidity in USD
pub const DEFAULT_MIN_LIQUIDITY_USD: f64 = 5_000.0;

/// Maximum number of snapshots to retain
pub const MAX_SNAPSHOTS: usize = 100;

/// Window size for trend calculation (in snapshots)
pub const TREND_WINDOW_SIZE: usize = 10;

#[derive(Error, Debug, Clone)]
pub enum LiquidityGuardError {
    #[error("Insufficient liquidity: ${0:.2} below minimum ${1:.2}")]
    InsufficientLiquidity(f64, f64),

    #[error("Emergency exit triggered: liquidity dropped {0:.1}%")]
    EmergencyExitTriggered(f64),

    #[error("No liquidity data available")]
    NoLiquidityData,
}

/// Trend direction for liquidity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LiquidityTrend {
    Rising,
    Stable,
    Falling,
    CriticalFalling,
}

impl LiquidityTrend {
    /// Returns true if the trend indicates potential danger
    pub fn is_concerning(&self) -> bool {
        matches!(self, LiquidityTrend::Falling | LiquidityTrend::CriticalFalling)
    }

    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            LiquidityTrend::Rising => "Liquidity is increasing",
            LiquidityTrend::Stable => "Liquidity is stable",
            LiquidityTrend::Falling => "Liquidity is decreasing",
            LiquidityTrend::CriticalFalling => "CRITICAL: Liquidity rapidly decreasing",
        }
    }
}

/// A liquidity snapshot at a point in time
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LiquiditySnapshot {
    /// Unix timestamp
    pub timestamp: u64,
    /// Liquidity in USD
    pub liquidity_usd: f64,
}

/// Liquidity monitoring status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityStatus {
    /// Current liquidity in USD
    pub current_liquidity: f64,
    /// Peak liquidity since monitoring started
    pub peak_liquidity: f64,
    /// Current trend
    pub trend: LiquidityTrend,
    /// Percentage change from peak
    pub pct_from_peak: f64,
    /// Whether emergency exit is recommended
    pub emergency_exit_recommended: bool,
    /// Number of snapshots recorded
    pub snapshot_count: usize,
}

/// Real-time liquidity monitoring guard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquidityGuard {
    /// Percentage drop that triggers emergency exit
    emergency_exit_threshold_pct: f64,
    /// Historical liquidity snapshots
    liquidity_snapshots: VecDeque<LiquiditySnapshot>,
    /// Minimum liquidity required in USD
    min_liquidity_usd: f64,
    /// Peak liquidity observed
    peak_liquidity: f64,
    /// Reference liquidity for comparison (e.g., entry point)
    reference_liquidity: Option<f64>,
}

impl Default for LiquidityGuard {
    fn default() -> Self {
        Self {
            emergency_exit_threshold_pct: DEFAULT_EMERGENCY_EXIT_THRESHOLD_PCT,
            liquidity_snapshots: VecDeque::with_capacity(MAX_SNAPSHOTS),
            min_liquidity_usd: DEFAULT_MIN_LIQUIDITY_USD,
            peak_liquidity: 0.0,
            reference_liquidity: None,
        }
    }
}

impl LiquidityGuard {
    /// Create a new liquidity guard with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a liquidity guard with custom settings
    pub fn with_config(
        emergency_exit_threshold_pct: f64,
        min_liquidity_usd: f64,
    ) -> Self {
        Self {
            emergency_exit_threshold_pct,
            liquidity_snapshots: VecDeque::with_capacity(MAX_SNAPSHOTS),
            min_liquidity_usd,
            peak_liquidity: 0.0,
            reference_liquidity: None,
        }
    }

    /// Set a reference point for liquidity (e.g., at position entry)
    pub fn set_reference(&mut self, liquidity: f64) {
        self.reference_liquidity = Some(liquidity);
    }

    /// Clear the reference point
    pub fn clear_reference(&mut self) {
        self.reference_liquidity = None;
    }

    /// Record a new liquidity snapshot
    pub fn record_liquidity(&mut self, timestamp: u64, liquidity: f64) {
        // Update peak
        if liquidity > self.peak_liquidity {
            self.peak_liquidity = liquidity;
        }

        // Add snapshot
        self.liquidity_snapshots.push_back(LiquiditySnapshot {
            timestamp,
            liquidity_usd: liquidity,
        });

        // Trim old snapshots
        while self.liquidity_snapshots.len() > MAX_SNAPSHOTS {
            self.liquidity_snapshots.pop_front();
        }

        tracing::debug!(
            "Liquidity recorded: ${:.2} (peak: ${:.2}, snapshots: {})",
            liquidity,
            self.peak_liquidity,
            self.liquidity_snapshots.len()
        );
    }

    /// Check if an emergency exit is required
    pub fn should_emergency_exit(&self, current: f64) -> bool {
        // Check against peak
        if self.peak_liquidity > 0.0 {
            let drop_pct = ((self.peak_liquidity - current) / self.peak_liquidity) * 100.0;
            if drop_pct >= self.emergency_exit_threshold_pct {
                tracing::warn!(
                    "EMERGENCY: Liquidity dropped {:.1}% from peak ${:.2} to ${:.2}",
                    drop_pct,
                    self.peak_liquidity,
                    current
                );
                return true;
            }
        }

        // Check against reference if set
        if let Some(ref_liq) = self.reference_liquidity {
            let drop_pct = ((ref_liq - current) / ref_liq) * 100.0;
            if drop_pct >= self.emergency_exit_threshold_pct {
                tracing::warn!(
                    "EMERGENCY: Liquidity dropped {:.1}% from reference ${:.2} to ${:.2}",
                    drop_pct,
                    ref_liq,
                    current
                );
                return true;
            }
        }

        // Check minimum threshold
        if current < self.min_liquidity_usd {
            tracing::warn!(
                "EMERGENCY: Liquidity ${:.2} below minimum ${:.2}",
                current,
                self.min_liquidity_usd
            );
            return true;
        }

        false
    }

    /// Calculate the current liquidity trend
    pub fn liquidity_trend(&self) -> LiquidityTrend {
        if self.liquidity_snapshots.len() < 2 {
            return LiquidityTrend::Stable;
        }

        // Get recent snapshots for trend calculation
        let window_size = TREND_WINDOW_SIZE.min(self.liquidity_snapshots.len());
        let recent: Vec<_> = self.liquidity_snapshots
            .iter()
            .rev()
            .take(window_size)
            .collect();

        if recent.len() < 2 {
            return LiquidityTrend::Stable;
        }

        // Calculate average rate of change
        let newest = recent[0].liquidity_usd;
        let oldest = recent[recent.len() - 1].liquidity_usd;

        if oldest == 0.0 {
            return LiquidityTrend::Stable;
        }

        let pct_change = ((newest - oldest) / oldest) * 100.0;

        // Classify trend
        if pct_change > 5.0 {
            LiquidityTrend::Rising
        } else if pct_change < -20.0 {
            LiquidityTrend::CriticalFalling
        } else if pct_change < -5.0 {
            LiquidityTrend::Falling
        } else {
            LiquidityTrend::Stable
        }
    }

    /// Get the current liquidity (most recent snapshot)
    pub fn current_liquidity(&self) -> Option<f64> {
        self.liquidity_snapshots.back().map(|s| s.liquidity_usd)
    }

    /// Get the peak liquidity observed
    pub fn peak_liquidity(&self) -> f64 {
        self.peak_liquidity
    }

    /// Calculate percentage drop from peak
    pub fn pct_from_peak(&self, current: f64) -> f64 {
        if self.peak_liquidity > 0.0 {
            ((self.peak_liquidity - current) / self.peak_liquidity) * 100.0
        } else {
            0.0
        }
    }

    /// Get comprehensive liquidity status
    pub fn status(&self) -> LiquidityStatus {
        let current = self.current_liquidity().unwrap_or(0.0);
        let trend = self.liquidity_trend();
        let pct_from_peak = self.pct_from_peak(current);

        LiquidityStatus {
            current_liquidity: current,
            peak_liquidity: self.peak_liquidity,
            trend,
            pct_from_peak,
            emergency_exit_recommended: self.should_emergency_exit(current),
            snapshot_count: self.liquidity_snapshots.len(),
        }
    }

    /// Reset the guard (clear all snapshots and peak)
    pub fn reset(&mut self) {
        self.liquidity_snapshots.clear();
        self.peak_liquidity = 0.0;
        self.reference_liquidity = None;
        tracing::info!("Liquidity guard reset");
    }

    /// Get the number of snapshots recorded
    pub fn snapshot_count(&self) -> usize {
        self.liquidity_snapshots.len()
    }

    /// Check if liquidity meets minimum requirements
    pub fn meets_minimum(&self, liquidity: f64) -> bool {
        liquidity >= self.min_liquidity_usd
    }

    /// Validate liquidity and return error if insufficient
    pub fn validate_liquidity(&self, liquidity: f64) -> Result<(), LiquidityGuardError> {
        if liquidity < self.min_liquidity_usd {
            return Err(LiquidityGuardError::InsufficientLiquidity(
                liquidity,
                self.min_liquidity_usd,
            ));
        }

        if self.should_emergency_exit(liquidity) {
            let drop_pct = self.pct_from_peak(liquidity);
            return Err(LiquidityGuardError::EmergencyExitTriggered(drop_pct));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_guard() {
        let guard = LiquidityGuard::new();
        assert_eq!(guard.emergency_exit_threshold_pct, DEFAULT_EMERGENCY_EXIT_THRESHOLD_PCT);
        assert_eq!(guard.min_liquidity_usd, DEFAULT_MIN_LIQUIDITY_USD);
        assert_eq!(guard.snapshot_count(), 0);
    }

    #[test]
    fn test_record_liquidity() {
        let mut guard = LiquidityGuard::new();
        guard.record_liquidity(1000, 50_000.0);
        guard.record_liquidity(1001, 55_000.0);

        assert_eq!(guard.snapshot_count(), 2);
        assert_eq!(guard.peak_liquidity(), 55_000.0);
        assert_eq!(guard.current_liquidity(), Some(55_000.0));
    }

    #[test]
    fn test_emergency_exit_from_peak() {
        let mut guard = LiquidityGuard::with_config(30.0, 1_000.0);
        guard.record_liquidity(1000, 100_000.0);

        // 25% drop - should NOT trigger
        assert!(!guard.should_emergency_exit(75_000.0));

        // 35% drop - should trigger
        assert!(guard.should_emergency_exit(65_000.0));
    }

    #[test]
    fn test_emergency_exit_from_reference() {
        let mut guard = LiquidityGuard::with_config(30.0, 1_000.0);
        guard.set_reference(100_000.0);

        // 35% drop from reference - should trigger
        assert!(guard.should_emergency_exit(65_000.0));
    }

    #[test]
    fn test_emergency_exit_below_minimum() {
        let guard = LiquidityGuard::with_config(50.0, 10_000.0);

        // Below minimum - should trigger
        assert!(guard.should_emergency_exit(5_000.0));

        // Above minimum - should NOT trigger (no peak set)
        assert!(!guard.should_emergency_exit(15_000.0));
    }

    #[test]
    fn test_liquidity_trend_rising() {
        let mut guard = LiquidityGuard::new();

        // Record increasing liquidity
        for i in 0..10 {
            guard.record_liquidity(1000 + i as u64, 50_000.0 + (i as f64 * 5_000.0));
        }

        assert_eq!(guard.liquidity_trend(), LiquidityTrend::Rising);
    }

    #[test]
    fn test_liquidity_trend_falling() {
        let mut guard = LiquidityGuard::new();

        // Record decreasing liquidity (more than 5%, less than 20%)
        for i in 0..10 {
            guard.record_liquidity(1000 + i as u64, 100_000.0 - (i as f64 * 1_500.0));
        }

        assert_eq!(guard.liquidity_trend(), LiquidityTrend::Falling);
    }

    #[test]
    fn test_liquidity_trend_critical_falling() {
        let mut guard = LiquidityGuard::new();

        // Record sharply decreasing liquidity (>20%)
        for i in 0..10 {
            guard.record_liquidity(1000 + i as u64, 100_000.0 - (i as f64 * 5_000.0));
        }

        assert_eq!(guard.liquidity_trend(), LiquidityTrend::CriticalFalling);
    }

    #[test]
    fn test_liquidity_trend_stable() {
        let mut guard = LiquidityGuard::new();

        // Record relatively stable liquidity
        for i in 0..10 {
            let variation = if i % 2 == 0 { 1_000.0 } else { -1_000.0 };
            guard.record_liquidity(1000 + i as u64, 50_000.0 + variation);
        }

        assert_eq!(guard.liquidity_trend(), LiquidityTrend::Stable);
    }

    #[test]
    fn test_pct_from_peak() {
        let mut guard = LiquidityGuard::new();
        guard.record_liquidity(1000, 100_000.0);

        assert_eq!(guard.pct_from_peak(100_000.0), 0.0);
        assert_eq!(guard.pct_from_peak(90_000.0), 10.0);
        assert_eq!(guard.pct_from_peak(50_000.0), 50.0);
    }

    #[test]
    fn test_status() {
        let mut guard = LiquidityGuard::new();
        guard.record_liquidity(1000, 100_000.0);
        guard.record_liquidity(1001, 95_000.0);

        let status = guard.status();
        assert_eq!(status.current_liquidity, 95_000.0);
        assert_eq!(status.peak_liquidity, 100_000.0);
        assert_eq!(status.snapshot_count, 2);
        assert!(!status.emergency_exit_recommended);
    }

    #[test]
    fn test_reset() {
        let mut guard = LiquidityGuard::new();
        guard.record_liquidity(1000, 100_000.0);
        guard.set_reference(90_000.0);

        guard.reset();

        assert_eq!(guard.snapshot_count(), 0);
        assert_eq!(guard.peak_liquidity(), 0.0);
        assert_eq!(guard.current_liquidity(), None);
    }

    #[test]
    fn test_meets_minimum() {
        let guard = LiquidityGuard::with_config(30.0, 10_000.0);

        assert!(guard.meets_minimum(15_000.0));
        assert!(guard.meets_minimum(10_000.0));
        assert!(!guard.meets_minimum(9_999.0));
    }

    #[test]
    fn test_validate_liquidity() {
        let mut guard = LiquidityGuard::with_config(30.0, 10_000.0);
        guard.record_liquidity(1000, 100_000.0);

        // Valid liquidity
        assert!(guard.validate_liquidity(80_000.0).is_ok());

        // Below minimum
        let result = guard.validate_liquidity(5_000.0);
        assert!(matches!(result, Err(LiquidityGuardError::InsufficientLiquidity(_, _))));

        // Emergency exit triggered
        let result = guard.validate_liquidity(60_000.0);
        assert!(matches!(result, Err(LiquidityGuardError::EmergencyExitTriggered(_))));
    }

    #[test]
    fn test_trend_concerning() {
        assert!(!LiquidityTrend::Rising.is_concerning());
        assert!(!LiquidityTrend::Stable.is_concerning());
        assert!(LiquidityTrend::Falling.is_concerning());
        assert!(LiquidityTrend::CriticalFalling.is_concerning());
    }

    #[test]
    fn test_max_snapshots() {
        let mut guard = LiquidityGuard::new();

        // Record more than MAX_SNAPSHOTS
        for i in 0..(MAX_SNAPSHOTS + 50) {
            guard.record_liquidity(1000 + i as u64, 50_000.0);
        }

        // Should be capped at MAX_SNAPSHOTS
        assert_eq!(guard.snapshot_count(), MAX_SNAPSHOTS);
    }

    #[test]
    fn test_trend_with_insufficient_data() {
        let guard = LiquidityGuard::new();
        assert_eq!(guard.liquidity_trend(), LiquidityTrend::Stable);

        let mut guard = LiquidityGuard::new();
        guard.record_liquidity(1000, 50_000.0);
        assert_eq!(guard.liquidity_trend(), LiquidityTrend::Stable);
    }

    #[test]
    fn test_reference_liquidity() {
        let mut guard = LiquidityGuard::with_config(30.0, 1_000.0);

        // No reference set, no peak - should not trigger
        assert!(!guard.should_emergency_exit(70_000.0));

        // Set reference
        guard.set_reference(100_000.0);

        // 35% drop from reference - should trigger
        assert!(guard.should_emergency_exit(65_000.0));

        // Clear reference
        guard.clear_reference();

        // Should not trigger anymore (no reference, no peak)
        assert!(!guard.should_emergency_exit(65_000.0));
    }
}
