//! Meme Coin Trading Configuration
//!
//! Configuration structures for the meme coin trading module.
//! Maps to the `[meme]` section in config.toml.

use serde::Deserialize;
use std::path::PathBuf;

use super::types::TokenEntry;

/// Meme trading configuration section
///
/// This struct maps to the `[meme]` section in config.toml and contains
/// all parameters for the multi-token meme coin trading strategy.
#[derive(Debug, Clone, Deserialize)]
pub struct MemeConfig {
    /// Enable meme trading module (if false, `butters meme` commands are disabled)
    #[serde(default)]
    pub enabled: bool,

    // =========================================================================
    // OU Process Parameters
    // =========================================================================
    /// Number of price samples for OU parameter estimation
    #[serde(default = "default_ou_lookback")]
    pub ou_lookback: usize,

    /// Time step between samples in minutes (should match poll_interval_secs / 60)
    #[serde(default = "default_ou_dt_minutes")]
    pub ou_dt_minutes: f64,

    /// Minimum confidence score (0-1) for OU parameters to be considered valid
    /// Higher = more conservative, requires better parameter fit
    #[serde(default = "default_min_ou_confidence")]
    pub min_ou_confidence: f64,

    /// Minimum half-life in minutes for a token to be tradeable
    /// Too short (<5min): Noise, not mean-reverting
    #[serde(default = "default_min_half_life_minutes")]
    pub min_half_life_minutes: f64,

    /// Maximum half-life in minutes for a token to be tradeable
    /// Too long (>120min): Too slow to profit from
    #[serde(default = "default_max_half_life_minutes")]
    pub max_half_life_minutes: f64,

    // =========================================================================
    // Entry/Exit Thresholds
    // =========================================================================
    /// Z-score threshold for entry (negative = oversold, buy opportunity)
    /// -3.5 = very conservative (0.02% of normal distribution)
    /// -2.5 = moderate (0.62% of normal distribution)
    #[serde(default = "default_z_entry_threshold")]
    pub z_entry_threshold: f64,

    /// Z-score threshold for exit (0.0 = exit at mean)
    /// Positive values = exit slightly above mean for momentum
    #[serde(default = "default_z_exit_threshold")]
    pub z_exit_threshold: f64,

    /// Stop loss percentage (triggers if price drops this much from entry)
    #[serde(default = "default_stop_loss_pct")]
    pub stop_loss_pct: f64,

    /// Take profit percentage (triggers if price rises this much from entry)
    #[serde(default = "default_take_profit_pct")]
    pub take_profit_pct: f64,

    /// Maximum position age in hours before forced exit
    /// Prevents capital being stuck in non-moving positions
    #[serde(default = "default_max_position_hours")]
    pub max_position_hours: f64,

    // =========================================================================
    // Trade Execution
    // =========================================================================
    /// Trade size in USDC per position
    #[serde(default = "default_trade_size_usdc")]
    pub trade_size_usdc: f64,

    /// Slippage tolerance in basis points (100 = 1%)
    /// Higher for volatile meme coins
    #[serde(default = "default_slippage_bps")]
    pub slippage_bps: u16,

    /// Poll interval in seconds for price updates
    #[serde(default = "default_poll_interval_secs")]
    pub poll_interval_secs: u64,

    /// Priority fee in lamports for transaction inclusion
    #[serde(default = "default_priority_fee_lamports")]
    pub priority_fee_lamports: u64,

    /// Maximum price impact allowed (percentage)
    /// Abort trade if Jupiter quote shows higher impact
    #[serde(default = "default_max_price_impact_pct")]
    pub max_price_impact_pct: f64,

    // =========================================================================
    // Safety & Persistence
    // =========================================================================
    /// Enable paper trading mode (no real transactions)
    #[serde(default = "default_paper_mode")]
    pub paper_mode: bool,

    /// Data directory for position persistence and state
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,

    // =========================================================================
    // Initial Token List
    // =========================================================================
    /// Tokens to track on startup (can also use CLI to add/remove)
    #[serde(default)]
    pub tokens: Vec<TokenEntry>,
}

// Default value functions
fn default_ou_lookback() -> usize {
    100
}
fn default_ou_dt_minutes() -> f64 {
    1.0
}
fn default_min_ou_confidence() -> f64 {
    0.3
}
fn default_min_half_life_minutes() -> f64 {
    5.0
}
fn default_max_half_life_minutes() -> f64 {
    120.0
}
fn default_z_entry_threshold() -> f64 {
    -3.5
}
fn default_z_exit_threshold() -> f64 {
    0.0
}
fn default_stop_loss_pct() -> f64 {
    10.0
}
fn default_take_profit_pct() -> f64 {
    15.0
}
fn default_max_position_hours() -> f64 {
    4.0
}
fn default_trade_size_usdc() -> f64 {
    50.0
}
fn default_slippage_bps() -> u16 {
    100
}
fn default_poll_interval_secs() -> u64 {
    60
}
fn default_priority_fee_lamports() -> u64 {
    10_000
}
fn default_max_price_impact_pct() -> f64 {
    2.0
}
fn default_paper_mode() -> bool {
    true
}
fn default_data_dir() -> PathBuf {
    PathBuf::from("data/meme")
}

impl Default for MemeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ou_lookback: default_ou_lookback(),
            ou_dt_minutes: default_ou_dt_minutes(),
            min_ou_confidence: default_min_ou_confidence(),
            min_half_life_minutes: default_min_half_life_minutes(),
            max_half_life_minutes: default_max_half_life_minutes(),
            z_entry_threshold: default_z_entry_threshold(),
            z_exit_threshold: default_z_exit_threshold(),
            stop_loss_pct: default_stop_loss_pct(),
            take_profit_pct: default_take_profit_pct(),
            max_position_hours: default_max_position_hours(),
            trade_size_usdc: default_trade_size_usdc(),
            slippage_bps: default_slippage_bps(),
            poll_interval_secs: default_poll_interval_secs(),
            priority_fee_lamports: default_priority_fee_lamports(),
            max_price_impact_pct: default_max_price_impact_pct(),
            paper_mode: default_paper_mode(),
            data_dir: default_data_dir(),
            tokens: vec![],
        }
    }
}

impl MemeConfig {
    /// Validate the meme configuration
    pub fn validate(&self) -> Result<(), MemeConfigError> {
        // OU parameters
        if self.ou_lookback == 0 {
            return Err(MemeConfigError::InvalidValue(
                "ou_lookback must be > 0".to_string(),
            ));
        }

        if self.ou_dt_minutes <= 0.0 {
            return Err(MemeConfigError::InvalidValue(
                "ou_dt_minutes must be > 0".to_string(),
            ));
        }

        if self.min_ou_confidence < 0.0 || self.min_ou_confidence > 1.0 {
            return Err(MemeConfigError::InvalidValue(
                "min_ou_confidence must be 0.0 - 1.0".to_string(),
            ));
        }

        if self.min_half_life_minutes <= 0.0 {
            return Err(MemeConfigError::InvalidValue(
                "min_half_life_minutes must be > 0".to_string(),
            ));
        }

        if self.max_half_life_minutes <= self.min_half_life_minutes {
            return Err(MemeConfigError::InvalidValue(
                "max_half_life_minutes must be > min_half_life_minutes".to_string(),
            ));
        }

        // Entry/Exit thresholds
        if self.z_entry_threshold >= 0.0 {
            return Err(MemeConfigError::InvalidValue(
                "z_entry_threshold must be < 0 (negative for oversold)".to_string(),
            ));
        }

        if self.stop_loss_pct <= 0.0 || self.stop_loss_pct > 100.0 {
            return Err(MemeConfigError::InvalidValue(
                "stop_loss_pct must be 0 - 100".to_string(),
            ));
        }

        if self.take_profit_pct <= 0.0 || self.take_profit_pct > 100.0 {
            return Err(MemeConfigError::InvalidValue(
                "take_profit_pct must be 0 - 100".to_string(),
            ));
        }

        if self.max_position_hours <= 0.0 {
            return Err(MemeConfigError::InvalidValue(
                "max_position_hours must be > 0".to_string(),
            ));
        }

        // Trade execution
        if self.trade_size_usdc <= 0.0 {
            return Err(MemeConfigError::InvalidValue(
                "trade_size_usdc must be > 0".to_string(),
            ));
        }

        if self.slippage_bps == 0 {
            return Err(MemeConfigError::InvalidValue(
                "slippage_bps must be > 0".to_string(),
            ));
        }

        if self.poll_interval_secs == 0 {
            return Err(MemeConfigError::InvalidValue(
                "poll_interval_secs must be > 0".to_string(),
            ));
        }

        if self.max_price_impact_pct <= 0.0 || self.max_price_impact_pct > 100.0 {
            return Err(MemeConfigError::InvalidValue(
                "max_price_impact_pct must be 0 - 100".to_string(),
            ));
        }

        // Validate token entries
        for (i, token) in self.tokens.iter().enumerate() {
            if token.mint.is_empty() {
                return Err(MemeConfigError::InvalidValue(format!(
                    "tokens[{}].mint cannot be empty",
                    i
                )));
            }
            if token.symbol.is_empty() {
                return Err(MemeConfigError::InvalidValue(format!(
                    "tokens[{}].symbol cannot be empty",
                    i
                )));
            }
        }

        Ok(())
    }

    /// Check if a half-life value is within tradeable range
    pub fn is_half_life_tradeable(&self, half_life_minutes: f64) -> bool {
        half_life_minutes >= self.min_half_life_minutes
            && half_life_minutes <= self.max_half_life_minutes
    }
}

/// Meme configuration errors
#[derive(Debug, Clone)]
pub enum MemeConfigError {
    /// Invalid configuration value
    InvalidValue(String),
}

impl std::fmt::Display for MemeConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MemeConfigError::InvalidValue(msg) => write!(f, "Invalid meme config: {}", msg),
        }
    }
}

impl std::error::Error for MemeConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MemeConfig::default();

        assert!(!config.enabled);
        assert_eq!(config.ou_lookback, 100);
        assert_eq!(config.z_entry_threshold, -3.5);
        assert_eq!(config.trade_size_usdc, 50.0);
        assert!(config.paper_mode);
        assert!(config.tokens.is_empty());
    }

    #[test]
    fn test_validate_valid_config() {
        let config = MemeConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_ou_lookback() {
        let mut config = MemeConfig::default();
        config.ou_lookback = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_z_entry_threshold() {
        let mut config = MemeConfig::default();
        config.z_entry_threshold = 1.0; // Must be negative
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_half_life_range() {
        let mut config = MemeConfig::default();
        config.min_half_life_minutes = 100.0;
        config.max_half_life_minutes = 50.0; // Less than min
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_is_half_life_tradeable() {
        let config = MemeConfig::default();

        // Within range
        assert!(config.is_half_life_tradeable(30.0));
        assert!(config.is_half_life_tradeable(5.0)); // At min
        assert!(config.is_half_life_tradeable(120.0)); // At max

        // Outside range
        assert!(!config.is_half_life_tradeable(4.0)); // Below min
        assert!(!config.is_half_life_tradeable(121.0)); // Above max
    }

    #[test]
    fn test_validate_empty_token_mint() {
        let mut config = MemeConfig::default();
        config.tokens.push(TokenEntry {
            mint: "".to_string(),
            symbol: "TEST".to_string(),
            decimals: 9,
        });
        assert!(config.validate().is_err());
    }
}
