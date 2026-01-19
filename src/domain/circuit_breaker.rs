//! Circuit Breaker
//!
//! Portfolio-level loss protection that halts trading when daily losses,
//! consecutive losses, or loss rates exceed thresholds.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use thiserror::Error;

/// Default maximum daily loss in USD
pub const DEFAULT_MAX_DAILY_LOSS_USD: f64 = 50.0;

/// Default maximum consecutive losses
pub const DEFAULT_MAX_CONSECUTIVE_LOSSES: u32 = 3;

/// Default maximum loss rate per hour in USD
pub const DEFAULT_MAX_LOSS_RATE_PER_HOUR: f64 = 20.0;

/// Default cooldown period in minutes
pub const DEFAULT_COOLDOWN_MINUTES: u64 = 60;

/// Maximum history entries to keep
pub const MAX_HISTORY_ENTRIES: usize = 100;

#[derive(Error, Debug, Clone)]
pub enum CircuitBreakerError {
    #[error("Circuit breaker tripped: daily loss ${0:.2} exceeds maximum ${1:.2}")]
    DailyLossExceeded(f64, f64),

    #[error("Circuit breaker tripped: {0} consecutive losses exceed maximum {1}")]
    ConsecutiveLossesExceeded(u32, u32),

    #[error("Circuit breaker tripped: hourly loss rate ${0:.2}/hr exceeds maximum ${1:.2}/hr")]
    HourlyLossRateExceeded(f64, f64),

    #[error("Trading halted - circuit breaker active, cooldown remaining: {0} minutes")]
    TradingHalted(u64),
}

/// Status of the circuit breaker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitBreakerStatus {
    /// Trading is allowed
    Active,
    /// Circuit breaker is tripped, trading halted
    Tripped,
    /// In cooldown period after being tripped
    Cooldown,
}

impl CircuitBreakerStatus {
    /// Returns true if trading is allowed
    pub fn can_trade(&self) -> bool {
        matches!(self, CircuitBreakerStatus::Active)
    }

    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            CircuitBreakerStatus::Active => "Trading active - all systems nominal",
            CircuitBreakerStatus::Tripped => "Circuit breaker TRIPPED - trading halted",
            CircuitBreakerStatus::Cooldown => "Cooldown period - waiting to resume trading",
        }
    }
}

/// A record of a trade's profit/loss
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeRecord {
    /// Unix timestamp of the trade
    pub timestamp: u64,
    /// Profit/loss in USD (negative = loss)
    pub pnl_usd: f64,
}

/// Detailed circuit breaker state for monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreakerState {
    /// Current status
    pub status: CircuitBreakerStatus,
    /// Total daily loss in USD
    pub daily_loss: f64,
    /// Current consecutive loss count
    pub consecutive_losses: u32,
    /// Current hourly loss rate
    pub hourly_loss_rate: f64,
    /// Time until cooldown ends (seconds), if applicable
    pub cooldown_remaining_secs: Option<u64>,
    /// Reason for trip, if tripped
    pub trip_reason: Option<String>,
    /// Total trades recorded today
    pub trades_today: usize,
}

/// Portfolio loss protection circuit breaker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// Maximum daily loss in USD
    max_daily_loss_usd: f64,
    /// Maximum consecutive losses before trip
    max_consecutive_losses: u32,
    /// Maximum loss rate per hour in USD
    max_loss_rate_per_hour: f64,
    /// Cooldown period after trip in minutes
    cooldown_minutes: u64,

    // State
    /// Accumulated daily loss (positive value = losses)
    daily_loss: f64,
    /// Current consecutive loss count
    consecutive_losses: u32,
    /// Recent losses with timestamps for rate calculation
    hourly_losses: VecDeque<TradeRecord>,
    /// Timestamp of last trade
    last_trade_time: Option<u64>,
    /// Whether circuit breaker is tripped
    is_tripped: bool,
    /// Timestamp when tripped (for cooldown calculation)
    tripped_at: Option<u64>,
    /// Day start timestamp for daily reset
    day_start: u64,
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self {
            max_daily_loss_usd: DEFAULT_MAX_DAILY_LOSS_USD,
            max_consecutive_losses: DEFAULT_MAX_CONSECUTIVE_LOSSES,
            max_loss_rate_per_hour: DEFAULT_MAX_LOSS_RATE_PER_HOUR,
            cooldown_minutes: DEFAULT_COOLDOWN_MINUTES,
            daily_loss: 0.0,
            consecutive_losses: 0,
            hourly_losses: VecDeque::with_capacity(MAX_HISTORY_ENTRIES),
            last_trade_time: None,
            is_tripped: false,
            tripped_at: None,
            day_start: 0,
        }
    }
}

impl CircuitBreaker {
    /// Create a new circuit breaker with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a circuit breaker with custom settings
    pub fn with_config(
        max_daily_loss_usd: f64,
        max_consecutive_losses: u32,
        max_loss_rate_per_hour: f64,
        cooldown_minutes: u64,
    ) -> Self {
        Self {
            max_daily_loss_usd,
            max_consecutive_losses,
            max_loss_rate_per_hour,
            cooldown_minutes,
            ..Self::default()
        }
    }

    /// Record a trade and update circuit breaker state
    pub fn record_trade(&mut self, pnl_usd: f64, timestamp: u64) {
        // Check for day rollover
        self.check_day_rollover(timestamp);

        // Record the trade
        let record = TradeRecord { timestamp, pnl_usd };
        self.hourly_losses.push_back(record);

        // Trim old entries (keep only last hour)
        let one_hour_ago = timestamp.saturating_sub(3600);
        while self.hourly_losses.front().map_or(false, |r| r.timestamp < one_hour_ago) {
            self.hourly_losses.pop_front();
        }

        // Update consecutive losses
        if pnl_usd < 0.0 {
            self.consecutive_losses += 1;
            self.daily_loss += pnl_usd.abs();
            tracing::debug!(
                "Loss recorded: ${:.2} (consecutive: {}, daily: ${:.2})",
                pnl_usd.abs(),
                self.consecutive_losses,
                self.daily_loss
            );
        } else {
            self.consecutive_losses = 0;
            tracing::debug!("Win recorded: ${:.2} (consecutive losses reset)", pnl_usd);
        }

        self.last_trade_time = Some(timestamp);

        // Check trip conditions
        self.check_trip_conditions(timestamp);
    }

    /// Check if trading is allowed
    pub fn can_trade(&self) -> bool {
        !self.is_tripped
    }

    /// Check if trading is allowed, accounting for cooldown
    pub fn can_trade_at(&self, timestamp: u64) -> bool {
        if !self.is_tripped {
            return true;
        }

        // Check if cooldown has passed
        if let Some(tripped_at) = self.tripped_at {
            let cooldown_secs = self.cooldown_minutes * 60;
            if timestamp >= tripped_at + cooldown_secs {
                return true;
            }
        }

        false
    }

    /// Get the current status
    pub fn status(&self) -> CircuitBreakerStatus {
        if !self.is_tripped {
            CircuitBreakerStatus::Active
        } else if self.tripped_at.is_some() {
            CircuitBreakerStatus::Cooldown
        } else {
            CircuitBreakerStatus::Tripped
        }
    }

    /// Get detailed state for monitoring
    pub fn state(&self, current_timestamp: u64) -> CircuitBreakerState {
        let cooldown_remaining = if self.is_tripped {
            self.tripped_at.map(|t| {
                let cooldown_end = t + (self.cooldown_minutes * 60);
                cooldown_end.saturating_sub(current_timestamp)
            })
        } else {
            None
        };

        let trip_reason = if self.is_tripped {
            Some(self.get_trip_reason())
        } else {
            None
        };

        CircuitBreakerState {
            status: self.status(),
            daily_loss: self.daily_loss,
            consecutive_losses: self.consecutive_losses,
            hourly_loss_rate: self.calculate_hourly_loss_rate(),
            cooldown_remaining_secs: cooldown_remaining,
            trip_reason,
            trades_today: self.hourly_losses.len(),
        }
    }

    /// Manually reset the circuit breaker
    pub fn reset(&mut self) {
        self.daily_loss = 0.0;
        self.consecutive_losses = 0;
        self.hourly_losses.clear();
        self.is_tripped = false;
        self.tripped_at = None;
        tracing::info!("Circuit breaker reset");
    }

    /// Try to auto-reset after cooldown
    pub fn try_auto_reset(&mut self, current_timestamp: u64) -> bool {
        if self.can_trade_at(current_timestamp) && self.is_tripped {
            tracing::info!("Circuit breaker cooldown complete, auto-resetting");
            self.is_tripped = false;
            self.tripped_at = None;
            // Keep daily loss, only reset consecutive
            self.consecutive_losses = 0;
            return true;
        }
        false
    }

    /// Get remaining cooldown time in minutes
    pub fn cooldown_remaining(&self, current_timestamp: u64) -> Option<u64> {
        if !self.is_tripped {
            return None;
        }

        self.tripped_at.map(|t| {
            let cooldown_end = t + (self.cooldown_minutes * 60);
            let remaining_secs = cooldown_end.saturating_sub(current_timestamp);
            remaining_secs / 60
        })
    }

    /// Get the daily loss amount
    pub fn daily_loss(&self) -> f64 {
        self.daily_loss
    }

    /// Get the consecutive loss count
    pub fn consecutive_losses(&self) -> u32 {
        self.consecutive_losses
    }

    /// Calculate current hourly loss rate
    fn calculate_hourly_loss_rate(&self) -> f64 {
        self.hourly_losses
            .iter()
            .filter(|r| r.pnl_usd < 0.0)
            .map(|r| r.pnl_usd.abs())
            .sum()
    }

    /// Check and apply trip conditions
    fn check_trip_conditions(&mut self, timestamp: u64) {
        if self.is_tripped {
            return;
        }

        // Check daily loss
        if self.daily_loss >= self.max_daily_loss_usd {
            self.trip(timestamp);
            tracing::error!(
                "CIRCUIT BREAKER TRIPPED: Daily loss ${:.2} exceeds maximum ${:.2}",
                self.daily_loss,
                self.max_daily_loss_usd
            );
            return;
        }

        // Check consecutive losses
        if self.consecutive_losses >= self.max_consecutive_losses {
            self.trip(timestamp);
            tracing::error!(
                "CIRCUIT BREAKER TRIPPED: {} consecutive losses exceed maximum {}",
                self.consecutive_losses,
                self.max_consecutive_losses
            );
            return;
        }

        // Check hourly loss rate
        let hourly_rate = self.calculate_hourly_loss_rate();
        if hourly_rate >= self.max_loss_rate_per_hour {
            self.trip(timestamp);
            tracing::error!(
                "CIRCUIT BREAKER TRIPPED: Hourly loss rate ${:.2}/hr exceeds maximum ${:.2}/hr",
                hourly_rate,
                self.max_loss_rate_per_hour
            );
        }
    }

    /// Trip the circuit breaker
    fn trip(&mut self, timestamp: u64) {
        self.is_tripped = true;
        self.tripped_at = Some(timestamp);
    }

    /// Get the reason for the trip
    fn get_trip_reason(&self) -> String {
        if self.daily_loss >= self.max_daily_loss_usd {
            format!(
                "Daily loss ${:.2} exceeds maximum ${:.2}",
                self.daily_loss, self.max_daily_loss_usd
            )
        } else if self.consecutive_losses >= self.max_consecutive_losses {
            format!(
                "{} consecutive losses exceed maximum {}",
                self.consecutive_losses, self.max_consecutive_losses
            )
        } else {
            let hourly_rate = self.calculate_hourly_loss_rate();
            format!(
                "Hourly loss rate ${:.2}/hr exceeds maximum ${:.2}/hr",
                hourly_rate, self.max_loss_rate_per_hour
            )
        }
    }

    /// Check for day rollover and reset daily counters
    fn check_day_rollover(&mut self, timestamp: u64) {
        // Calculate current day start (midnight UTC)
        let day_seconds = 86400u64;
        let current_day_start = (timestamp / day_seconds) * day_seconds;

        if current_day_start > self.day_start {
            tracing::info!("Day rollover detected, resetting daily loss counter");
            self.day_start = current_day_start;
            self.daily_loss = 0.0;
            // Don't reset consecutive losses - they span days
        }
    }

    /// Validate if a trade can proceed
    pub fn validate_trade(&self, timestamp: u64) -> Result<(), CircuitBreakerError> {
        if !self.can_trade_at(timestamp) {
            if let Some(remaining) = self.cooldown_remaining(timestamp) {
                return Err(CircuitBreakerError::TradingHalted(remaining));
            } else {
                return Err(CircuitBreakerError::TradingHalted(0));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_breaker() -> CircuitBreaker {
        CircuitBreaker::with_config(50.0, 3, 20.0, 60)
    }

    #[test]
    fn test_new_breaker() {
        let breaker = CircuitBreaker::new();
        assert!(breaker.can_trade());
        assert_eq!(breaker.status(), CircuitBreakerStatus::Active);
        assert_eq!(breaker.daily_loss(), 0.0);
        assert_eq!(breaker.consecutive_losses(), 0);
    }

    #[test]
    fn test_record_win() {
        let mut breaker = create_test_breaker();
        breaker.record_trade(10.0, 1000);

        assert!(breaker.can_trade());
        assert_eq!(breaker.consecutive_losses(), 0);
        assert_eq!(breaker.daily_loss(), 0.0);
    }

    #[test]
    fn test_record_loss() {
        let mut breaker = create_test_breaker();
        breaker.record_trade(-10.0, 1000);

        assert!(breaker.can_trade());
        assert_eq!(breaker.consecutive_losses(), 1);
        assert_eq!(breaker.daily_loss(), 10.0);
    }

    #[test]
    fn test_consecutive_loss_trip() {
        let mut breaker = create_test_breaker();

        // Record 3 consecutive losses
        breaker.record_trade(-5.0, 1000);
        breaker.record_trade(-5.0, 1001);
        assert!(breaker.can_trade()); // Still can trade

        breaker.record_trade(-5.0, 1002);
        assert!(!breaker.can_trade()); // Should be tripped
        assert_eq!(breaker.status(), CircuitBreakerStatus::Cooldown);
    }

    #[test]
    fn test_consecutive_loss_reset_on_win() {
        let mut breaker = create_test_breaker();

        // Record 2 losses then a win
        breaker.record_trade(-5.0, 1000);
        breaker.record_trade(-5.0, 1001);
        assert_eq!(breaker.consecutive_losses(), 2);

        breaker.record_trade(5.0, 1002);
        assert_eq!(breaker.consecutive_losses(), 0);
        assert!(breaker.can_trade());
    }

    #[test]
    fn test_daily_loss_trip() {
        // Create breaker with high hourly rate limit to test daily loss specifically
        let mut breaker = CircuitBreaker::with_config(50.0, 10, 100.0, 60);

        // Record losses totaling $50
        breaker.record_trade(-25.0, 1000);
        assert!(breaker.can_trade());

        breaker.record_trade(-25.0, 1001);
        assert!(!breaker.can_trade());
    }

    #[test]
    fn test_hourly_rate_trip() {
        let mut breaker = CircuitBreaker::with_config(100.0, 10, 20.0, 60);

        // Record losses totaling $20+ in rapid succession
        breaker.record_trade(-10.0, 1000);
        breaker.record_trade(-15.0, 1001);

        assert!(!breaker.can_trade());
    }

    #[test]
    fn test_cooldown() {
        let mut breaker = create_test_breaker();

        // Trip the breaker
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);
        assert!(!breaker.can_trade());

        // Check during cooldown
        assert!(!breaker.can_trade_at(1002 + 30 * 60)); // 30 minutes

        // Check after cooldown
        assert!(breaker.can_trade_at(1002 + 61 * 60)); // 61 minutes
    }

    #[test]
    fn test_auto_reset() {
        let mut breaker = create_test_breaker();

        // Trip the breaker
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);

        // Try reset before cooldown
        assert!(!breaker.try_auto_reset(1002 + 30 * 60));
        assert!(!breaker.can_trade());

        // Try reset after cooldown
        assert!(breaker.try_auto_reset(1002 + 61 * 60));
        assert!(breaker.can_trade());
    }

    #[test]
    fn test_manual_reset() {
        let mut breaker = create_test_breaker();

        // Trip the breaker
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);
        assert!(!breaker.can_trade());

        // Manual reset
        breaker.reset();
        assert!(breaker.can_trade());
        assert_eq!(breaker.daily_loss(), 0.0);
        assert_eq!(breaker.consecutive_losses(), 0);
    }

    #[test]
    fn test_cooldown_remaining() {
        let mut breaker = create_test_breaker();

        // Trip the breaker
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);

        // Check cooldown remaining
        let remaining = breaker.cooldown_remaining(1002 + 30 * 60);
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > 0);

        // After cooldown
        let remaining = breaker.cooldown_remaining(1002 + 61 * 60);
        assert_eq!(remaining, Some(0));
    }

    #[test]
    fn test_state() {
        let mut breaker = create_test_breaker();
        breaker.record_trade(-10.0, 1000);
        breaker.record_trade(-5.0, 1001);

        let state = breaker.state(1002);
        assert_eq!(state.status, CircuitBreakerStatus::Active);
        assert_eq!(state.daily_loss, 15.0);
        assert_eq!(state.consecutive_losses, 2);
        assert!(state.cooldown_remaining_secs.is_none());
        assert!(state.trip_reason.is_none());
    }

    #[test]
    fn test_state_when_tripped() {
        let mut breaker = create_test_breaker();
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);

        let state = breaker.state(1002 + 30 * 60);
        assert_eq!(state.status, CircuitBreakerStatus::Cooldown);
        assert!(state.cooldown_remaining_secs.is_some());
        assert!(state.trip_reason.is_some());
    }

    #[test]
    fn test_validate_trade() {
        let mut breaker = create_test_breaker();

        // Should pass when active
        assert!(breaker.validate_trade(1000).is_ok());

        // Trip the breaker
        breaker.record_trade(-20.0, 1000);
        breaker.record_trade(-20.0, 1001);
        breaker.record_trade(-20.0, 1002);

        // Should fail when tripped
        let result = breaker.validate_trade(1003);
        assert!(matches!(result, Err(CircuitBreakerError::TradingHalted(_))));
    }

    #[test]
    fn test_day_rollover() {
        let mut breaker = create_test_breaker();

        // Record losses on day 1
        let day1 = 86400u64; // Midnight of day 1
        breaker.record_trade(-30.0, day1 + 1000);
        assert_eq!(breaker.daily_loss(), 30.0);

        // Record on day 2 - should reset daily loss
        let day2 = day1 + 86400;
        breaker.record_trade(-10.0, day2 + 1000);
        assert_eq!(breaker.daily_loss(), 10.0); // Reset for new day
    }

    #[test]
    fn test_hourly_loss_cleanup() {
        let mut breaker = create_test_breaker();

        // Record a loss at time 0
        breaker.record_trade(-5.0, 0);

        // Record more losses over an hour later
        breaker.record_trade(-5.0, 3601);
        breaker.record_trade(-5.0, 3602);

        // Old loss should be cleaned up
        assert_eq!(breaker.hourly_losses.len(), 2);
    }

    #[test]
    fn test_status_description() {
        assert!(CircuitBreakerStatus::Active.can_trade());
        assert!(!CircuitBreakerStatus::Tripped.can_trade());
        assert!(!CircuitBreakerStatus::Cooldown.can_trade());

        assert!(CircuitBreakerStatus::Active.description().contains("active"));
        assert!(CircuitBreakerStatus::Tripped.description().contains("TRIPPED"));
    }

    #[test]
    fn test_custom_config() {
        let breaker = CircuitBreaker::with_config(100.0, 5, 50.0, 30);

        assert_eq!(breaker.max_daily_loss_usd, 100.0);
        assert_eq!(breaker.max_consecutive_losses, 5);
        assert_eq!(breaker.max_loss_rate_per_hour, 50.0);
        assert_eq!(breaker.cooldown_minutes, 30);
    }
}
