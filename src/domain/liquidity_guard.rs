//! Liquidity Guard
//!
//! Real-time liquidity monitoring for meme coin trading.
//! Detects significant LP changes and potential rug conditions.
//!
//! Key features:
//! - Emergency exit threshold monitoring
//! - Trend detection (rising/stable/falling/critical)
//! - Historical snapshot tracking
//! - Organic dip vs rug pull classification

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Default minimum liquidity in USD
pub const DEFAULT_MIN_LIQUIDITY_USD: f64 = 1_000.0;

/// Default emergency exit threshold (30% drop from reference)
pub const DEFAULT_EMERGENCY_EXIT_PCT: f64 = 0.30;

/// Default rapid drop threshold for rug detection (50% in short window)
pub const DEFAULT_RAPID_DROP_PCT: f64 = 0.50;

/// Default rapid drop time window in seconds
pub const DEFAULT_RAPID_DROP_WINDOW_SECS: u64 = 30;

/// Maximum history snapshots to keep
pub const MAX_HISTORY_SIZE: usize = 100;

/// Errors for liquidity guard
#[derive(Error, Debug, Clone)]
pub enum LiquidityGuardError {
    #[error("Liquidity below minimum: {current} USD < {minimum} USD")]
    BelowMinimum { current: f64, minimum: f64 },

    #[error("Emergency exit triggered: {drop_pct:.1}% drop exceeds {threshold_pct:.1}% threshold")]
    EmergencyExit { drop_pct: f64, threshold_pct: f64 },

    #[error("Rug pull detected: {drop_pct:.1}% drop in {seconds} seconds")]
    RugPullDetected { drop_pct: f64, seconds: u64 },

    #[error("No reference point set - call set_reference first")]
    NoReference,

    #[error("Insufficient history for trend analysis")]
    InsufficientHistory,
}

/// Liquidity trend classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityTrend {
    /// Liquidity increasing
    Rising,
    /// Liquidity stable (within 5%)
    Stable,
    /// Liquidity declining gradually
    Falling,
    /// Liquidity dropping rapidly - potential rug
    CriticalFalling,
}

impl LiquidityTrend {
    /// Whether this trend is concerning
    pub fn is_concerning(&self) -> bool {
        matches!(self, LiquidityTrend::Falling | LiquidityTrend::CriticalFalling)
    }

    /// Whether this trend indicates imminent danger
    pub fn is_critical(&self) -> bool {
        matches!(self, LiquidityTrend::CriticalFalling)
    }
}

/// Classification of liquidity events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityEvent {
    /// Normal organic price movement
    OrganicDip,
    /// Likely rug pull in progress
    RugPullLikely,
    /// Confirmed rug pull (rapid massive drop)
    RugPullConfirmed,
}

/// Exit decision from liquidity analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitDecision {
    /// Hold position - situation is safe
    Hold,
    /// Exit partial position (percentage)
    ExitPartial(u8),
    /// Exit immediately - emergency
    ExitImmediate,
}

/// Point-in-time liquidity snapshot
#[derive(Debug, Clone)]
pub struct LiquiditySnapshot {
    /// Liquidity in USD
    pub liquidity_usd: f64,
    /// Timestamp (unix seconds)
    pub timestamp: u64,
    /// Optional: slot number
    pub slot: Option<u64>,
}

impl LiquiditySnapshot {
    pub fn new(liquidity_usd: f64) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        Self {
            liquidity_usd,
            timestamp,
            slot: None,
        }
    }

    pub fn with_slot(mut self, slot: u64) -> Self {
        self.slot = Some(slot);
        self
    }

    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
}

/// Configuration for liquidity guard
#[derive(Debug, Clone)]
pub struct LiquidityGuardConfig {
    /// Minimum liquidity required (USD)
    pub min_liquidity_usd: f64,

    /// Emergency exit threshold (percentage drop from reference)
    pub emergency_exit_threshold_pct: f64,

    /// Rapid drop threshold for rug detection
    pub rapid_drop_threshold_pct: f64,

    /// Time window for rapid drop detection (seconds)
    pub rapid_drop_window_seconds: u64,

    /// Organic dip tolerance (normal volatility)
    pub organic_dip_tolerance_pct: f64,

    /// Recovery window before declaring rug (seconds)
    pub recovery_window_seconds: u64,
}

impl Default for LiquidityGuardConfig {
    fn default() -> Self {
        Self {
            min_liquidity_usd: DEFAULT_MIN_LIQUIDITY_USD,
            emergency_exit_threshold_pct: DEFAULT_EMERGENCY_EXIT_PCT,
            rapid_drop_threshold_pct: DEFAULT_RAPID_DROP_PCT,
            rapid_drop_window_seconds: DEFAULT_RAPID_DROP_WINDOW_SECS,
            organic_dip_tolerance_pct: 0.20, // 20% dips are normal for memes
            recovery_window_seconds: 60,     // Wait 60s before declaring rug
        }
    }
}

impl LiquidityGuardConfig {
    /// Conservative config for graduated tokens
    pub fn graduated() -> Self {
        Self {
            min_liquidity_usd: 10_000.0, // $10K minimum
            emergency_exit_threshold_pct: 0.25, // 25% triggers exit
            rapid_drop_threshold_pct: 0.40,
            rapid_drop_window_seconds: 60,
            organic_dip_tolerance_pct: 0.15,
            recovery_window_seconds: 120,
        }
    }

    /// Lenient config for pump.fun sniping
    pub fn pump_fun_sniper() -> Self {
        Self {
            min_liquidity_usd: 500.0, // $500 minimum (very early)
            emergency_exit_threshold_pct: 0.40, // 40% triggers exit
            rapid_drop_threshold_pct: 0.60,
            rapid_drop_window_seconds: 20,
            organic_dip_tolerance_pct: 0.30, // 30% dips common
            recovery_window_seconds: 30,
        }
    }

    /// Custom config
    pub fn custom(
        min_liquidity_usd: f64,
        emergency_exit_pct: f64,
        rapid_drop_pct: f64,
    ) -> Self {
        Self {
            min_liquidity_usd,
            emergency_exit_threshold_pct: emergency_exit_pct,
            rapid_drop_threshold_pct: rapid_drop_pct,
            ..Default::default()
        }
    }
}

/// Alert level for liquidity changes
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertLevel {
    /// No concern
    Normal,
    /// Slight decline, monitor closely
    Watch,
    /// Significant decline, prepare exit
    Warning,
    /// Critical - exit recommended
    Critical,
    /// Emergency - exit immediately
    Emergency,
}

/// Liquidity guard for real-time monitoring
pub struct LiquidityGuard {
    /// Configuration
    config: LiquidityGuardConfig,
    /// Reference liquidity point (entry point)
    reference: Option<LiquiditySnapshot>,
    /// Historical snapshots (newest first)
    history: VecDeque<LiquiditySnapshot>,
    /// Current alert level
    alert_level: AlertLevel,
    /// Whether emergency exit has been triggered
    emergency_triggered: bool,
}

impl LiquidityGuard {
    /// Create a new liquidity guard
    pub fn new() -> Self {
        Self::with_config(LiquidityGuardConfig::default())
    }

    /// Create with custom config
    pub fn with_config(config: LiquidityGuardConfig) -> Self {
        Self {
            config,
            reference: None,
            history: VecDeque::with_capacity(MAX_HISTORY_SIZE),
            alert_level: AlertLevel::Normal,
            emergency_triggered: false,
        }
    }

    /// Create for graduated tokens
    pub fn graduated() -> Self {
        Self::with_config(LiquidityGuardConfig::graduated())
    }

    /// Create for pump.fun sniping
    pub fn pump_fun_sniper() -> Self {
        Self::with_config(LiquidityGuardConfig::pump_fun_sniper())
    }

    /// Set reference point (typically at entry)
    pub fn set_reference(&mut self, liquidity_usd: f64) {
        let snapshot = LiquiditySnapshot::new(liquidity_usd);
        self.reference = Some(snapshot.clone());
        self.add_snapshot_internal(snapshot);
        tracing::info!("Liquidity reference set: ${:.2}", liquidity_usd);
    }

    /// Set reference with slot
    pub fn set_reference_with_slot(&mut self, liquidity_usd: f64, slot: u64) {
        let snapshot = LiquiditySnapshot::new(liquidity_usd).with_slot(slot);
        self.reference = Some(snapshot.clone());
        self.add_snapshot_internal(snapshot);
        tracing::info!(
            "Liquidity reference set: ${:.2} at slot {}",
            liquidity_usd,
            slot
        );
    }

    /// Add a snapshot to history
    fn add_snapshot_internal(&mut self, snapshot: LiquiditySnapshot) {
        self.history.push_front(snapshot);
        if self.history.len() > MAX_HISTORY_SIZE {
            self.history.pop_back();
        }
    }

    /// Record a new liquidity observation
    pub fn record(&mut self, liquidity_usd: f64) -> Result<AlertLevel, LiquidityGuardError> {
        let snapshot = LiquiditySnapshot::new(liquidity_usd);
        self.add_snapshot_internal(snapshot);

        // Check minimum
        if liquidity_usd < self.config.min_liquidity_usd {
            self.alert_level = AlertLevel::Emergency;
            return Err(LiquidityGuardError::BelowMinimum {
                current: liquidity_usd,
                minimum: self.config.min_liquidity_usd,
            });
        }

        // Update alert level based on reference comparison
        self.alert_level = self.calculate_alert_level(liquidity_usd);

        Ok(self.alert_level)
    }

    /// Calculate alert level based on current vs reference
    fn calculate_alert_level(&self, current: f64) -> AlertLevel {
        let Some(ref reference) = self.reference else {
            return AlertLevel::Normal;
        };

        let drop_pct = (reference.liquidity_usd - current) / reference.liquidity_usd;

        if drop_pct >= self.config.emergency_exit_threshold_pct {
            AlertLevel::Emergency
        } else if drop_pct >= self.config.emergency_exit_threshold_pct * 0.75 {
            AlertLevel::Critical
        } else if drop_pct >= self.config.organic_dip_tolerance_pct {
            AlertLevel::Warning
        } else if drop_pct >= self.config.organic_dip_tolerance_pct * 0.5 {
            AlertLevel::Watch
        } else {
            AlertLevel::Normal
        }
    }

    /// Check if emergency exit should be triggered
    pub fn check_emergency(&mut self, current_liquidity: f64) -> Result<(), LiquidityGuardError> {
        let Some(ref reference) = self.reference else {
            return Err(LiquidityGuardError::NoReference);
        };

        let drop_pct = (reference.liquidity_usd - current_liquidity) / reference.liquidity_usd;

        if drop_pct >= self.config.emergency_exit_threshold_pct {
            self.emergency_triggered = true;
            tracing::error!(
                "EMERGENCY: Liquidity dropped {:.1}% from reference (${:.2} -> ${:.2})",
                drop_pct * 100.0,
                reference.liquidity_usd,
                current_liquidity
            );
            return Err(LiquidityGuardError::EmergencyExit {
                drop_pct: drop_pct * 100.0,
                threshold_pct: self.config.emergency_exit_threshold_pct * 100.0,
            });
        }

        Ok(())
    }

    /// Detect rapid drops that indicate rug pull
    pub fn detect_rug_pull(&self, current_liquidity: f64) -> Result<(), LiquidityGuardError> {
        if self.history.len() < 2 {
            return Ok(());
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Look back within the rapid drop window
        for snapshot in &self.history {
            let age = now.saturating_sub(snapshot.timestamp);
            if age > self.config.rapid_drop_window_seconds {
                break;
            }

            let drop_pct = (snapshot.liquidity_usd - current_liquidity) / snapshot.liquidity_usd;
            if drop_pct >= self.config.rapid_drop_threshold_pct {
                tracing::error!(
                    "RUG PULL DETECTED: {:.1}% drop in {} seconds",
                    drop_pct * 100.0,
                    age
                );
                return Err(LiquidityGuardError::RugPullDetected {
                    drop_pct: drop_pct * 100.0,
                    seconds: age,
                });
            }
        }

        Ok(())
    }

    /// Classify the current liquidity event
    pub fn classify_event(&self, current_liquidity: f64) -> LiquidityEvent {
        // Check for rapid drop (rug pull confirmed)
        if self.detect_rug_pull(current_liquidity).is_err() {
            return LiquidityEvent::RugPullConfirmed;
        }

        // Check emergency threshold (rug pull likely)
        if let Some(ref reference) = self.reference {
            let drop_pct = (reference.liquidity_usd - current_liquidity) / reference.liquidity_usd;
            if drop_pct >= self.config.emergency_exit_threshold_pct {
                return LiquidityEvent::RugPullLikely;
            }
        }

        LiquidityEvent::OrganicDip
    }

    /// Get exit decision based on current state
    pub fn should_exit(&self, current_liquidity: f64) -> ExitDecision {
        match self.classify_event(current_liquidity) {
            LiquidityEvent::OrganicDip => ExitDecision::Hold,
            LiquidityEvent::RugPullLikely => ExitDecision::ExitPartial(50),
            LiquidityEvent::RugPullConfirmed => ExitDecision::ExitImmediate,
        }
    }

    /// Detect trend from recent history
    pub fn detect_trend(&self) -> Result<LiquidityTrend, LiquidityGuardError> {
        if self.history.len() < 3 {
            return Err(LiquidityGuardError::InsufficientHistory);
        }

        // Compare recent snapshots
        let recent: Vec<_> = self.history.iter().take(5).collect();
        let oldest = recent.last().unwrap().liquidity_usd;
        let newest = recent.first().unwrap().liquidity_usd;

        let change_pct = (newest - oldest) / oldest;

        // Check for rapid changes
        if change_pct < -0.20 {
            // 20% drop = critical
            Ok(LiquidityTrend::CriticalFalling)
        } else if change_pct < -0.05 {
            // 5% drop = falling
            Ok(LiquidityTrend::Falling)
        } else if change_pct > 0.05 {
            // 5% rise = rising
            Ok(LiquidityTrend::Rising)
        } else {
            Ok(LiquidityTrend::Stable)
        }
    }

    /// Get current alert level
    pub fn alert_level(&self) -> AlertLevel {
        self.alert_level
    }

    /// Check if emergency has been triggered
    pub fn is_emergency_triggered(&self) -> bool {
        self.emergency_triggered
    }

    /// Get reference liquidity
    pub fn reference(&self) -> Option<&LiquiditySnapshot> {
        self.reference.as_ref()
    }

    /// Get history
    pub fn history(&self) -> &VecDeque<LiquiditySnapshot> {
        &self.history
    }

    /// Get config
    pub fn config(&self) -> &LiquidityGuardConfig {
        &self.config
    }

    /// Reset the guard (typically after exiting a position)
    pub fn reset(&mut self) {
        self.reference = None;
        self.history.clear();
        self.alert_level = AlertLevel::Normal;
        self.emergency_triggered = false;
        tracing::info!("Liquidity guard reset");
    }

    /// Get percentage drop from reference
    pub fn drop_from_reference(&self, current: f64) -> Option<f64> {
        self.reference.as_ref().map(|r| {
            ((r.liquidity_usd - current) / r.liquidity_usd * 100.0).max(0.0)
        })
    }
}

impl Default for LiquidityGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LiquidityGuardConfig::default();
        assert_eq!(config.min_liquidity_usd, DEFAULT_MIN_LIQUIDITY_USD);
        assert_eq!(config.emergency_exit_threshold_pct, DEFAULT_EMERGENCY_EXIT_PCT);
    }

    #[test]
    fn test_set_reference() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        assert!(guard.reference().is_some());
        assert_eq!(guard.reference().unwrap().liquidity_usd, 10_000.0);
    }

    #[test]
    fn test_record_normal() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let result = guard.record(9_500.0); // 5% drop
        assert!(result.is_ok());
        // 5% drop is within normal range (default organic_dip_tolerance is 20%)
        // 5% is less than 10% (50% of 20%), so it's Normal
        assert_eq!(result.unwrap(), AlertLevel::Normal);
    }

    #[test]
    fn test_record_below_minimum() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let result = guard.record(500.0); // Below $1000 minimum
        assert!(matches!(result, Err(LiquidityGuardError::BelowMinimum { .. })));
    }

    #[test]
    fn test_emergency_exit_triggered() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let result = guard.check_emergency(6_000.0); // 40% drop
        assert!(matches!(result, Err(LiquidityGuardError::EmergencyExit { .. })));
        assert!(guard.is_emergency_triggered());
    }

    #[test]
    fn test_emergency_not_triggered() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let result = guard.check_emergency(8_000.0); // 20% drop (within threshold)
        assert!(result.is_ok());
        assert!(!guard.is_emergency_triggered());
    }

    #[test]
    fn test_classify_organic_dip() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let event = guard.classify_event(8_500.0); // 15% drop
        assert_eq!(event, LiquidityEvent::OrganicDip);
    }

    #[test]
    fn test_classify_rug_likely() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let event = guard.classify_event(6_500.0); // 35% drop
        assert_eq!(event, LiquidityEvent::RugPullLikely);
    }

    #[test]
    fn test_exit_decision_hold() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let decision = guard.should_exit(9_000.0);
        assert_eq!(decision, ExitDecision::Hold);
    }

    #[test]
    fn test_exit_decision_partial() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let decision = guard.should_exit(6_500.0); // 35% drop
        assert_eq!(decision, ExitDecision::ExitPartial(50));
    }

    #[test]
    fn test_alert_levels_progression() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        // Default thresholds:
        // - emergency_exit_threshold_pct = 0.30 (30%)
        // - organic_dip_tolerance_pct = 0.20 (20%)
        // Alert levels:
        // - >= 30% = Emergency
        // - >= 22.5% (75% of 30%) = Critical
        // - >= 20% = Warning
        // - >= 10% (50% of 20%) = Watch
        // - < 10% = Normal

        // Normal (2% drop)
        let _ = guard.record(9_800.0);
        assert_eq!(guard.alert_level(), AlertLevel::Normal);

        // Watch (10% drop)
        let _ = guard.record(9_000.0);
        assert_eq!(guard.alert_level(), AlertLevel::Watch);

        // Warning (21% drop, >= 20% but < 22.5%)
        let _ = guard.record(7_900.0);
        assert_eq!(guard.alert_level(), AlertLevel::Warning);

        // Critical (25% drop, >= 22.5% but < 30%)
        let _ = guard.record(7_500.0);
        assert_eq!(guard.alert_level(), AlertLevel::Critical);

        // Emergency (35% drop, >= 30%)
        let _ = guard.record(6_500.0);
        assert_eq!(guard.alert_level(), AlertLevel::Emergency);
    }

    #[test]
    fn test_trend_detection_stable() {
        let mut guard = LiquidityGuard::new();

        // Add stable snapshots
        for i in 0..5 {
            let snapshot = LiquiditySnapshot::new(10_000.0 + (i as f64 * 100.0))
                .with_timestamp(1000 + i * 10);
            guard.history.push_front(snapshot);
        }

        let trend = guard.detect_trend().unwrap();
        assert!(matches!(trend, LiquidityTrend::Stable | LiquidityTrend::Rising));
    }

    #[test]
    fn test_trend_detection_critical_falling() {
        let mut guard = LiquidityGuard::new();

        // Add falling snapshots - push_front means newest first
        // So the order after all pushes will be: [7000, 8000, 9000, 10000, 10500] (newest first)
        // We take first 5, compare newest (7000) vs oldest (10500) = -33% = critical
        guard.history.push_front(LiquiditySnapshot::new(10500.0).with_timestamp(1010));
        guard.history.push_front(LiquiditySnapshot::new(10000.0).with_timestamp(1020));
        guard.history.push_front(LiquiditySnapshot::new(9000.0).with_timestamp(1030));
        guard.history.push_front(LiquiditySnapshot::new(8000.0).with_timestamp(1040));
        guard.history.push_front(LiquiditySnapshot::new(7000.0).with_timestamp(1050)); // newest

        let trend = guard.detect_trend().unwrap();
        // From oldest (10500) to newest (7000) = -33% = critical falling
        assert_eq!(trend, LiquidityTrend::CriticalFalling);
    }

    #[test]
    fn test_insufficient_history() {
        let guard = LiquidityGuard::new();
        let result = guard.detect_trend();
        assert!(matches!(result, Err(LiquidityGuardError::InsufficientHistory)));
    }

    #[test]
    fn test_reset() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);
        let _ = guard.record(5_000.0);
        let _ = guard.check_emergency(5_000.0);

        assert!(guard.is_emergency_triggered());

        guard.reset();

        assert!(guard.reference().is_none());
        assert!(!guard.is_emergency_triggered());
        assert_eq!(guard.alert_level(), AlertLevel::Normal);
    }

    #[test]
    fn test_drop_from_reference() {
        let mut guard = LiquidityGuard::new();
        guard.set_reference(10_000.0);

        let drop = guard.drop_from_reference(8_000.0);
        assert!(drop.is_some());
        assert!((drop.unwrap() - 20.0).abs() < 0.01); // 20% drop
    }

    #[test]
    fn test_pump_fun_sniper_config() {
        let config = LiquidityGuardConfig::pump_fun_sniper();
        assert_eq!(config.min_liquidity_usd, 500.0);
        assert_eq!(config.emergency_exit_threshold_pct, 0.40);
    }

    #[test]
    fn test_graduated_config() {
        let config = LiquidityGuardConfig::graduated();
        assert_eq!(config.min_liquidity_usd, 10_000.0);
        assert_eq!(config.emergency_exit_threshold_pct, 0.25);
    }

    #[test]
    fn test_history_size_limit() {
        let mut guard = LiquidityGuard::new();

        // Add more than MAX_HISTORY_SIZE snapshots
        for i in 0..(MAX_HISTORY_SIZE + 20) {
            let snapshot = LiquiditySnapshot::new(10_000.0).with_timestamp(i as u64);
            guard.history.push_front(snapshot);
            if guard.history.len() > MAX_HISTORY_SIZE {
                guard.history.pop_back();
            }
        }

        assert_eq!(guard.history.len(), MAX_HISTORY_SIZE);
    }
}
