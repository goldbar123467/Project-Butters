//! Strategy Parameters
//!
//! Configuration structs for the mean reversion strategy.
//! Default values target ~1.5% trigger frequency.

use serde::{Deserialize, Serialize};

/// Main strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    /// Number of candles for rolling statistics
    pub lookback_period: usize,
    /// Z-score threshold for entry signals (e.g., 2.5 = 2.5 std devs)
    pub z_threshold: f64,
    /// Minimum seconds between trades
    pub cooldown_seconds: u64,
    /// Risk management settings
    pub risk: RiskConfig,
    /// Market filters
    pub filters: FilterConfig,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            lookback_period: 50,
            z_threshold: 2.5,
            cooldown_seconds: 300, // 5 minutes
            risk: RiskConfig::default(),
            filters: FilterConfig::default(),
        }
    }
}

impl StrategyConfig {
    /// Create a new config with custom z-threshold
    pub fn with_z_threshold(mut self, threshold: f64) -> Self {
        self.z_threshold = threshold;
        self
    }

    /// Create a new config with custom lookback period
    pub fn with_lookback(mut self, period: usize) -> Self {
        self.lookback_period = period;
        self
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.lookback_period < 10 {
            return Err(ConfigError::InvalidLookback(self.lookback_period));
        }
        if self.z_threshold <= 0.0 || self.z_threshold > 5.0 {
            return Err(ConfigError::InvalidZThreshold(self.z_threshold));
        }
        self.risk.validate()?;
        self.filters.validate()?;
        Ok(())
    }
}

/// Risk management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskConfig {
    /// Maximum position size as percentage of portfolio
    pub max_position_pct: f64,
    /// Stop loss percentage
    pub stop_loss_pct: f64,
    /// Take profit percentage
    pub take_profit_pct: f64,
    /// Maximum trades per day
    pub max_daily_trades: u32,
    /// Maximum daily loss as percentage of portfolio
    pub max_daily_loss_pct: f64,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            max_position_pct: 5.0,
            stop_loss_pct: 2.0,
            take_profit_pct: 1.5,
            max_daily_trades: 10,
            max_daily_loss_pct: 3.0,
        }
    }
}

impl RiskConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_position_pct <= 0.0 || self.max_position_pct > 100.0 {
            return Err(ConfigError::InvalidPositionSize(self.max_position_pct));
        }
        if self.stop_loss_pct <= 0.0 || self.stop_loss_pct > 50.0 {
            return Err(ConfigError::InvalidStopLoss(self.stop_loss_pct));
        }
        if self.take_profit_pct <= 0.0 || self.take_profit_pct > 100.0 {
            return Err(ConfigError::InvalidTakeProfit(self.take_profit_pct));
        }
        Ok(())
    }
}

/// Market filter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Minimum volume percentile to consider (0-100)
    pub min_volume_percentile: f64,
    /// Maximum spread in basis points
    pub max_spread_bps: u32,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            min_volume_percentile: 60.0,
            max_spread_bps: 30,
        }
    }
}

impl FilterConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.min_volume_percentile < 0.0 || self.min_volume_percentile > 100.0 {
            return Err(ConfigError::InvalidVolumeFilter(self.min_volume_percentile));
        }
        if self.max_spread_bps > 500 {
            return Err(ConfigError::InvalidSpreadFilter(self.max_spread_bps));
        }
        Ok(())
    }
}

/// Configuration validation errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid lookback period: {0} (minimum 10)")]
    InvalidLookback(usize),
    #[error("Invalid z-threshold: {0} (must be 0 < z <= 5)")]
    InvalidZThreshold(f64),
    #[error("Invalid position size: {0}% (must be 0 < size <= 100)")]
    InvalidPositionSize(f64),
    #[error("Invalid stop loss: {0}% (must be 0 < loss <= 50)")]
    InvalidStopLoss(f64),
    #[error("Invalid take profit: {0}% (must be 0 < profit <= 100)")]
    InvalidTakeProfit(f64),
    #[error("Invalid volume filter: {0} (must be 0-100)")]
    InvalidVolumeFilter(f64),
    #[error("Invalid spread filter: {0} bps (max 500)")]
    InvalidSpreadFilter(u32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = StrategyConfig::default();
        assert_eq!(config.lookback_period, 50);
        assert_eq!(config.z_threshold, 2.5);
        assert_eq!(config.cooldown_seconds, 300);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_builder() {
        let config = StrategyConfig::default()
            .with_z_threshold(2.0)
            .with_lookback(30);
        assert_eq!(config.z_threshold, 2.0);
        assert_eq!(config.lookback_period, 30);
    }

    #[test]
    fn test_invalid_lookback() {
        let mut config = StrategyConfig::default();
        config.lookback_period = 5;
        assert!(matches!(config.validate(), Err(ConfigError::InvalidLookback(5))));
    }

    #[test]
    fn test_invalid_z_threshold() {
        let mut config = StrategyConfig::default();
        config.z_threshold = 0.0;
        assert!(matches!(config.validate(), Err(ConfigError::InvalidZThreshold(_))));

        config.z_threshold = 6.0;
        assert!(matches!(config.validate(), Err(ConfigError::InvalidZThreshold(_))));
    }

    #[test]
    fn test_risk_config_validation() {
        let mut risk = RiskConfig::default();
        assert!(risk.validate().is_ok());

        risk.max_position_pct = 0.0;
        assert!(risk.validate().is_err());
    }

    #[test]
    fn test_filter_config_validation() {
        let mut filters = FilterConfig::default();
        assert!(filters.validate().is_ok());

        filters.min_volume_percentile = 150.0;
        assert!(filters.validate().is_err());
    }
}
