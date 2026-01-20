//! Meme Coin Trading Types
//!
//! Shared types for the meme coin trading module including token information,
//! position tracking, and OU process parameters.

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// Token entry from configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenEntry {
    /// Token mint address (base58)
    pub mint: String,
    /// Token symbol (e.g., "BONK", "WIF")
    pub symbol: String,
    /// Token decimals
    pub decimals: u8,
}

/// Runtime token information with metadata
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// Token mint address (base58)
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Token decimals
    pub decimals: u8,
    /// Current price in USDC
    pub price: Option<f64>,
    /// Current z-score from OU process
    pub z_score: Option<f64>,
    /// Half-life in minutes from OU process
    pub half_life_minutes: Option<f64>,
    /// Whether token meets tradeability criteria
    pub is_tradeable: bool,
}

impl TokenInfo {
    /// Create a new TokenInfo from a TokenEntry
    pub fn from_entry(entry: &TokenEntry) -> Self {
        Self {
            mint: entry.mint.clone(),
            symbol: entry.symbol.clone(),
            decimals: entry.decimals,
            price: None,
            z_score: None,
            half_life_minutes: None,
            is_tradeable: false,
        }
    }
}

/// OU process parameters for a token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OUParams {
    /// Long-term mean (mu)
    pub mu: f64,
    /// Volatility (sigma)
    pub sigma: f64,
    /// Mean reversion speed (theta)
    pub theta: f64,
    /// Parameter estimation confidence (0.0 - 1.0)
    pub confidence: f64,
}

impl OUParams {
    /// Calculate half-life in minutes from theta
    /// half_life = ln(2) / theta
    pub fn half_life_minutes(&self, dt_minutes: f64) -> f64 {
        if self.theta <= 0.0 {
            f64::INFINITY
        } else {
            (2.0_f64.ln() / self.theta) * dt_minutes
        }
    }
}

/// Active position in a meme token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemePosition {
    /// Token mint address
    pub token_mint: String,
    /// Token symbol
    pub token_symbol: String,
    /// Entry price in USDC
    pub entry_price: f64,
    /// Token amount held
    pub size: u64,
    /// USDC value at entry
    pub entry_value_usdc: f64,
    /// Entry timestamp (Unix seconds)
    pub entry_timestamp: u64,
    /// Z-score at entry
    pub entry_z_score: f64,
    /// OU parameters at entry time
    pub ou_params: OUParams,
}

impl MemePosition {
    /// Calculate current profit/loss percentage
    pub fn pnl_pct(&self, current_price: f64) -> f64 {
        if self.entry_price <= 0.0 {
            return 0.0;
        }
        ((current_price - self.entry_price) / self.entry_price) * 100.0
    }

    /// Calculate current value in USDC
    pub fn current_value_usdc(&self, current_price: f64, decimals: u8) -> f64 {
        let size_float = self.size as f64 / 10_u64.pow(decimals as u32) as f64;
        size_float * current_price
    }

    /// Calculate position age in hours
    pub fn age_hours(&self) -> f64 {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        (now.saturating_sub(self.entry_timestamp)) as f64 / 3600.0
    }
}

/// Persisted state for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    /// Active position if any
    pub active_position: Option<MemePosition>,
    /// Last update timestamp (Unix seconds)
    pub last_updated: u64,
    /// Wallet public key
    pub wallet: String,
}

impl PersistedState {
    /// Create new persisted state
    pub fn new(wallet: String) -> Self {
        Self {
            active_position: None,
            last_updated: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            wallet,
        }
    }
}

/// Entry signal for a meme token
#[derive(Debug, Clone)]
pub struct MemeEntrySignal {
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Current price
    pub price: f64,
    /// Z-score triggering entry
    pub z_score: f64,
    /// OU parameters
    pub ou_params: OUParams,
    /// Signal confidence (0.0 - 1.0)
    pub confidence: f64,
}

/// Exit signal for current position
#[derive(Debug, Clone)]
pub enum MemeExitReason {
    /// Z-score returned to mean
    MeanReversion { current_z_score: f64 },
    /// Stop loss triggered
    StopLoss { loss_pct: f64 },
    /// Take profit triggered
    TakeProfit { profit_pct: f64 },
    /// Position held too long
    TimeStop { hours_held: f64 },
    /// Manual exit requested
    Manual,
    /// Graceful shutdown
    Shutdown,
}

/// Trade execution result
#[derive(Debug, Clone)]
pub struct MemeTradeResult {
    /// Transaction signature
    pub signature: String,
    /// Amount of tokens bought/sold
    pub token_amount: u64,
    /// USDC amount spent/received
    pub usdc_amount: f64,
    /// Actual slippage in basis points
    pub actual_slippage_bps: u16,
    /// Execution price
    pub price: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_info_from_entry() {
        let entry = TokenEntry {
            mint: "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263".to_string(),
            symbol: "BONK".to_string(),
            decimals: 5,
        };

        let info = TokenInfo::from_entry(&entry);
        assert_eq!(info.mint, entry.mint);
        assert_eq!(info.symbol, "BONK");
        assert_eq!(info.decimals, 5);
        assert!(!info.is_tradeable);
    }

    #[test]
    fn test_ou_params_half_life() {
        let params = OUParams {
            mu: 0.000025,
            sigma: 0.00000042,
            theta: 0.038,
            confidence: 0.72,
        };

        // half_life = ln(2) / theta * dt
        // with dt=1: ln(2) / 0.038 = ~18.24
        let half_life = params.half_life_minutes(1.0);
        assert!((half_life - 18.24).abs() < 0.1);
    }

    #[test]
    fn test_meme_position_pnl() {
        let position = MemePosition {
            token_mint: "test".to_string(),
            token_symbol: "TEST".to_string(),
            entry_price: 100.0,
            size: 1000,
            entry_value_usdc: 100.0,
            entry_timestamp: 0,
            entry_z_score: -3.5,
            ou_params: OUParams {
                mu: 100.0,
                sigma: 5.0,
                theta: 0.1,
                confidence: 0.8,
            },
        };

        // 10% profit
        let pnl = position.pnl_pct(110.0);
        assert!((pnl - 10.0).abs() < 0.001);

        // 5% loss
        let pnl = position.pnl_pct(95.0);
        assert!((pnl - (-5.0)).abs() < 0.001);
    }

    #[test]
    fn test_persisted_state_new() {
        let state = PersistedState::new("wallet123".to_string());
        assert!(state.active_position.is_none());
        assert_eq!(state.wallet, "wallet123");
        assert!(state.last_updated > 0);
    }
}
