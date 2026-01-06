//! Configuration Loader
//!
//! Loads and validates configuration from TOML files matching config.toml structure.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Main configuration structure matching config.toml
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub strategy: StrategySection,
    pub risk: RiskSection,
    pub tokens: TokensSection,
    pub jupiter: JupiterSection,
    pub solana: SolanaSection,
    pub logging: LoggingSection,
    #[serde(default)]
    pub alerts: AlertsSection,
}

/// Strategy configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct StrategySection {
    /// Lookback period for rolling mean/std calculation (in candles)
    pub lookback_period: usize,
    /// Z-score threshold for entry (2.0 = moderate, 2.5 = conservative)
    pub z_threshold: f64,
    /// Exit at mean (0.0) or slight overshoot (0.5)
    pub z_exit_threshold: f64,
    /// Minimum volume percentile to trade (filter low-liquidity moments)
    pub min_volume_percentile: f64,
    /// Maximum spread in basis points (0.3% = 30 bps)
    pub max_spread_bps: u32,
    /// Cooldown between trades in seconds
    pub cooldown_seconds: u64,
    /// Timeframe for candles ("4h" or "1d")
    pub timeframe: String,
}

/// Risk management configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct RiskSection {
    /// Maximum position size as percentage of portfolio
    pub max_position_pct: f64,
    /// Stop loss percentage
    pub stop_loss_pct: f64,
    /// Take profit percentage (exit at mean reversion)
    pub take_profit_pct: f64,
    /// Maximum trades per day
    pub max_daily_trades: u32,
    /// Maximum daily loss percentage (circuit breaker)
    pub max_daily_loss_pct: f64,
    /// Time-based stop (exit after N hours if no movement)
    pub time_stop_hours: u64,
}

/// Tokens configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct TokensSection {
    /// SOL mint address
    pub base_mint: String,
    /// USDC mint address
    pub quote_mint: String,
    /// Trading pair symbol (for logging)
    pub pair_symbol: String,
}

/// Jupiter API configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct JupiterSection {
    /// Jupiter V6 API base URL
    pub api_url: String,
    /// Optional API key for higher rate limits (get from jup.ag)
    #[serde(default)]
    pub api_key: Option<String>,
    /// Slippage tolerance in basis points (0.5% = 50 bps)
    pub slippage_bps: u16,
    /// Restrict intermediate tokens to high-liquidity paths
    pub restrict_intermediate_tokens: bool,
    /// Priority fee mode: "auto", "high", "veryHigh"
    pub priority_level: String,
    /// Maximum priority fee in lamports (0.005 SOL cap)
    pub max_priority_fee_lamports: u64,
    /// Use dynamic compute unit limits
    pub dynamic_compute_units: bool,
}

/// Solana RPC configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct SolanaSection {
    /// RPC endpoint (use private RPC for production)
    pub rpc_url: String,
    /// Commitment level: "processed", "confirmed", "finalized"
    pub commitment: String,
    /// Wallet keypair path (NEVER commit this file!)
    pub keypair_path: String,
}

impl SolanaSection {
    /// Get RPC URL with environment variable override
    /// Checks SOLANA_RPC_URL env var first, falls back to config value
    pub fn get_rpc_url(&self) -> String {
        std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| self.rpc_url.clone())
    }

    /// Get keypair path with environment variable override
    /// Checks SOLANA_KEYPAIR_PATH env var first, falls back to config value
    pub fn get_keypair_path(&self) -> String {
        std::env::var("SOLANA_KEYPAIR_PATH").unwrap_or_else(|_| self.keypair_path.clone())
    }
}

/// Logging configuration section
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingSection {
    /// Log level: "trace", "debug", "info", "warn", "error"
    pub level: String,
    /// Log to file (in addition to stdout)
    pub log_to_file: bool,
    /// Log file path
    pub log_file: String,
}

/// Alerts configuration section (optional)
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AlertsSection {
    /// Enable Discord webhook notifications
    #[serde(default)]
    pub discord_enabled: bool,
    /// Discord webhook URL
    #[serde(default)]
    pub discord_webhook_url: String,
    /// Enable Telegram notifications
    #[serde(default)]
    pub telegram_enabled: bool,
    /// Telegram bot token
    #[serde(default)]
    pub telegram_bot_token: String,
    /// Telegram chat ID
    #[serde(default)]
    pub telegram_chat_id: String,
}

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    ParseError(#[from] toml::de::Error),
    #[error("Validation failed: {0}")]
    ValidationError(String),
}

/// Load configuration from a TOML file
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    config.validate()?;
    Ok(config)
}

impl Config {
    /// Validate all configuration parameters
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate strategy section
        if self.strategy.z_threshold <= 0.0 {
            return Err(ConfigError::ValidationError(format!(
                "z_threshold must be > 0, got {}",
                self.strategy.z_threshold
            )));
        }

        if self.strategy.lookback_period == 0 {
            return Err(ConfigError::ValidationError(format!(
                "lookback_period must be > 0, got {}",
                self.strategy.lookback_period
            )));
        }

        if self.strategy.min_volume_percentile < 0.0
            || self.strategy.min_volume_percentile > 100.0
        {
            return Err(ConfigError::ValidationError(format!(
                "min_volume_percentile must be 0-100, got {}",
                self.strategy.min_volume_percentile
            )));
        }

        // Validate risk section
        if self.risk.max_position_pct <= 0.0 || self.risk.max_position_pct > 100.0 {
            return Err(ConfigError::ValidationError(format!(
                "max_position_pct must be 0-100, got {}",
                self.risk.max_position_pct
            )));
        }

        if self.risk.stop_loss_pct <= 0.0 || self.risk.stop_loss_pct > 100.0 {
            return Err(ConfigError::ValidationError(format!(
                "stop_loss_pct must be 0-100, got {}",
                self.risk.stop_loss_pct
            )));
        }

        if self.risk.take_profit_pct <= 0.0 || self.risk.take_profit_pct > 100.0 {
            return Err(ConfigError::ValidationError(format!(
                "take_profit_pct must be 0-100, got {}",
                self.risk.take_profit_pct
            )));
        }

        if self.risk.max_daily_loss_pct <= 0.0 || self.risk.max_daily_loss_pct > 100.0 {
            return Err(ConfigError::ValidationError(format!(
                "max_daily_loss_pct must be 0-100, got {}",
                self.risk.max_daily_loss_pct
            )));
        }

        // Validate tokens
        if self.tokens.base_mint.is_empty() {
            return Err(ConfigError::ValidationError(
                "base_mint cannot be empty".to_string(),
            ));
        }

        if self.tokens.quote_mint.is_empty() {
            return Err(ConfigError::ValidationError(
                "quote_mint cannot be empty".to_string(),
            ));
        }

        // Validate Jupiter
        if self.jupiter.api_url.is_empty() {
            return Err(ConfigError::ValidationError(
                "api_url cannot be empty".to_string(),
            ));
        }

        // Validate Solana
        if self.solana.rpc_url.is_empty() {
            return Err(ConfigError::ValidationError(
                "rpc_url cannot be empty".to_string(),
            ));
        }

        if self.solana.keypair_path.is_empty() {
            return Err(ConfigError::ValidationError(
                "keypair_path cannot be empty".to_string(),
            ));
        }

        Ok(())
    }
}

impl JupiterSection {
    /// Get API key with environment variable fallback
    /// Checks JUPITER_API_KEY env var if config value is empty/None
    pub fn get_api_key(&self) -> Option<String> {
        // First check config value
        if let Some(ref key) = self.api_key {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }
        // Fall back to environment variable
        std::env::var("JUPITER_API_KEY").ok()
    }
}

// Conversion from Config to StrategyConfig
impl From<&Config> for crate::strategy::params::StrategyConfig {
    fn from(config: &Config) -> Self {
        use crate::strategy::params::{FilterConfig, RiskConfig, StrategyConfig};

        StrategyConfig {
            lookback_period: config.strategy.lookback_period,
            z_threshold: config.strategy.z_threshold,
            cooldown_seconds: config.strategy.cooldown_seconds,
            risk: RiskConfig {
                max_position_pct: config.risk.max_position_pct,
                stop_loss_pct: config.risk.stop_loss_pct,
                take_profit_pct: config.risk.take_profit_pct,
                max_daily_trades: config.risk.max_daily_trades,
                max_daily_loss_pct: config.risk.max_daily_loss_pct,
            },
            filters: FilterConfig {
                min_volume_percentile: config.strategy.min_volume_percentile,
                max_spread_bps: config.strategy.max_spread_bps,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_valid_config() -> String {
        r#"
[strategy]
lookback_period = 20
z_threshold = 2.0
z_exit_threshold = 0.0
min_volume_percentile = 60.0
max_spread_bps = 30
cooldown_seconds = 300
timeframe = "4h"

[risk]
max_position_pct = 5.0
stop_loss_pct = 2.5
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
time_stop_hours = 24

[tokens]
base_mint = "So11111111111111111111111111111111111111112"
quote_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
pair_symbol = "SOL/USDC"

[jupiter]
api_url = "https://public.jupiterapi.com"
slippage_bps = 50
restrict_intermediate_tokens = true
priority_level = "high"
max_priority_fee_lamports = 5000000
dynamic_compute_units = true

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "confirmed"
keypair_path = "~/.config/solana/id.json"

[logging]
level = "info"
log_to_file = true
log_file = "logs/butters.log"

[alerts]
discord_enabled = false
discord_webhook_url = ""
telegram_enabled = false
telegram_bot_token = ""
telegram_chat_id = ""
"#
        .to_string()
    }

    #[test]
    fn test_load_valid_config() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(create_valid_config().as_bytes()).unwrap();

        let config = load_config(file.path()).unwrap();

        assert_eq!(config.strategy.lookback_period, 20);
        assert_eq!(config.strategy.z_threshold, 2.0);
        assert_eq!(config.risk.max_position_pct, 5.0);
        assert_eq!(config.tokens.pair_symbol, "SOL/USDC");
    }

    #[test]
    fn test_load_missing_file() {
        let result = load_config("/nonexistent/path/config.toml");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::IoError(_)));
    }

    #[test]
    fn test_invalid_z_threshold() {
        let invalid_config = r#"
[strategy]
lookback_period = 20
z_threshold = 0.0
z_exit_threshold = 0.0
min_volume_percentile = 60.0
max_spread_bps = 30
cooldown_seconds = 300
timeframe = "4h"

[risk]
max_position_pct = 5.0
stop_loss_pct = 2.5
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
time_stop_hours = 24

[tokens]
base_mint = "So11111111111111111111111111111111111111112"
quote_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
pair_symbol = "SOL/USDC"

[jupiter]
api_url = "https://public.jupiterapi.com"
slippage_bps = 50
restrict_intermediate_tokens = true
priority_level = "high"
max_priority_fee_lamports = 5000000
dynamic_compute_units = true

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "confirmed"
keypair_path = "~/.config/solana/id.json"

[logging]
level = "info"
log_to_file = true
log_file = "logs/butters.log"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(invalid_config.as_bytes()).unwrap();

        let result = load_config(file.path());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::ValidationError(_)
        ));
    }

    #[test]
    fn test_invalid_lookback() {
        let invalid_config = r#"
[strategy]
lookback_period = 0
z_threshold = 2.0
z_exit_threshold = 0.0
min_volume_percentile = 60.0
max_spread_bps = 30
cooldown_seconds = 300
timeframe = "4h"

[risk]
max_position_pct = 5.0
stop_loss_pct = 2.5
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
time_stop_hours = 24

[tokens]
base_mint = "So11111111111111111111111111111111111111112"
quote_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
pair_symbol = "SOL/USDC"

[jupiter]
api_url = "https://public.jupiterapi.com"
slippage_bps = 50
restrict_intermediate_tokens = true
priority_level = "high"
max_priority_fee_lamports = 5000000
dynamic_compute_units = true

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "confirmed"
keypair_path = "~/.config/solana/id.json"

[logging]
level = "info"
log_to_file = true
log_file = "logs/butters.log"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(invalid_config.as_bytes()).unwrap();

        let result = load_config(file.path());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::ValidationError(_)
        ));
    }

    #[test]
    fn test_config_to_strategy_config() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(create_valid_config().as_bytes()).unwrap();

        let config = load_config(file.path()).unwrap();
        let strategy_config = crate::strategy::params::StrategyConfig::from(&config);

        assert_eq!(strategy_config.lookback_period, 20);
        assert_eq!(strategy_config.z_threshold, 2.0);
        assert_eq!(strategy_config.cooldown_seconds, 300);
        assert_eq!(strategy_config.risk.max_position_pct, 5.0);
        assert_eq!(strategy_config.risk.stop_loss_pct, 2.5);
        assert_eq!(strategy_config.filters.min_volume_percentile, 60.0);
        assert_eq!(strategy_config.filters.max_spread_bps, 30);
    }

    #[test]
    fn test_invalid_risk_percentages() {
        let invalid_config = r#"
[strategy]
lookback_period = 20
z_threshold = 2.0
z_exit_threshold = 0.0
min_volume_percentile = 60.0
max_spread_bps = 30
cooldown_seconds = 300
timeframe = "4h"

[risk]
max_position_pct = 150.0
stop_loss_pct = 2.5
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
time_stop_hours = 24

[tokens]
base_mint = "So11111111111111111111111111111111111111112"
quote_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
pair_symbol = "SOL/USDC"

[jupiter]
api_url = "https://public.jupiterapi.com"
slippage_bps = 50
restrict_intermediate_tokens = true
priority_level = "high"
max_priority_fee_lamports = 5000000
dynamic_compute_units = true

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "confirmed"
keypair_path = "~/.config/solana/id.json"

[logging]
level = "info"
log_to_file = true
log_file = "logs/butters.log"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(invalid_config.as_bytes()).unwrap();

        let result = load_config(file.path());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::ValidationError(_)
        ));
    }

    #[test]
    fn test_alerts_section_optional() {
        let config_without_alerts = r#"
[strategy]
lookback_period = 20
z_threshold = 2.0
z_exit_threshold = 0.0
min_volume_percentile = 60.0
max_spread_bps = 30
cooldown_seconds = 300
timeframe = "4h"

[risk]
max_position_pct = 5.0
stop_loss_pct = 2.5
take_profit_pct = 1.5
max_daily_trades = 10
max_daily_loss_pct = 3.0
time_stop_hours = 24

[tokens]
base_mint = "So11111111111111111111111111111111111111112"
quote_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
pair_symbol = "SOL/USDC"

[jupiter]
api_url = "https://public.jupiterapi.com"
slippage_bps = 50
restrict_intermediate_tokens = true
priority_level = "high"
max_priority_fee_lamports = 5000000
dynamic_compute_units = true

[solana]
rpc_url = "https://api.mainnet-beta.solana.com"
commitment = "confirmed"
keypair_path = "~/.config/solana/id.json"

[logging]
level = "info"
log_to_file = true
log_file = "logs/butters.log"
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(config_without_alerts.as_bytes()).unwrap();

        let config = load_config(file.path()).unwrap();
        assert!(!config.alerts.discord_enabled);
        assert!(!config.alerts.telegram_enabled);
    }
}
