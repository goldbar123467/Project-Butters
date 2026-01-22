//! Meme Coin Orchestrator
//!
//! Multi-token trading loop for meme coins using the OU-GBM strategy.
//! Key features:
//! - HashMap of OUProcess per token for signal generation
//! - Token discovery via Jupiter API
//! - Single active position at a time
//! - Always settles to USDC
//! - Graceful shutdown handling
//! - Trade lock for race condition prevention
//! - Position persistence for crash recovery
//!
//! Ported from butters-sniper for kyzlo-dex with adapted interfaces.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::adapters::jupiter::{JupiterClient, QuoteRequest, SwapRequest};
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::adapters::honeypot::SolanaHoneypotDetector;
use crate::domain::{BalanceGuard, ExpectedDelta};
use crate::domain::honeypot_detector::{HoneypotDetector, HoneypotRisk};
use crate::domain::liquidity_guard::{LiquidityGuard, LiquidityGuardConfig, LiquidityTrend};
use crate::domain::rug_detector::{RugDetector, RugDetectorConfig, RiskLevel, TokenSafetyReport};
use crate::strategy::ou_process::{OUProcess, OUSignal, OUParams};
use crate::strategy::regime::{
    MomentumAdxDetector, MomentumAdxConfig, MomentumSignal, CandleBuilder, Candle,
};

/// USDC mint address on Solana
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Default position persistence file
pub const POSITION_FILE: &str = "meme_position.json";

/// Minimum SOL balance to keep for transaction fees
pub const MIN_SOL_RESERVE_LAMPORTS: u64 = 50_000_000; // 0.05 SOL

/// Maximum price impact allowed (percentage)
pub const MAX_PRICE_IMPACT_PCT: f64 = 2.0;

#[derive(Debug, Error)]
pub enum MemeOrchestratorError {
    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Market data error: {0}")]
    MarketDataError(String),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Token not found: {0}")]
    TokenNotFound(String),

    #[error("Insufficient balance: have {have}, need {need}")]
    InsufficientBalance { have: u64, need: u64 },

    #[error("Position already open for {0}")]
    PositionAlreadyOpen(String),

    #[error("No position open")]
    NoPositionOpen,

    #[error("Trade lock acquisition failed")]
    TradeLockFailed,

    #[error("Trading halted: {0}")]
    TradingHalted(String),

    #[error("Price impact too high: {0}%")]
    PriceImpactTooHigh(f64),

    #[error("Persistence error: {0}")]
    PersistenceError(String),

    #[error("Shutdown requested")]
    ShutdownRequested,
}

/// Token info from discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token mint address
    pub mint: String,
    /// Token symbol (e.g., "BONK")
    pub symbol: String,
    /// Token name
    pub name: String,
    /// Decimals
    pub decimals: u8,
    /// Last known price in USDC
    pub price_usdc: Option<f64>,
    /// 24h volume in USDC
    pub volume_24h: Option<f64>,
    /// Market cap in USDC
    pub market_cap: Option<f64>,
    /// When this info was last updated
    pub last_updated: u64,
}

impl TokenInfo {
    pub fn new(mint: String, symbol: String, decimals: u8) -> Self {
        Self {
            mint,
            symbol,
            name: String::new(),
            decimals,
            price_usdc: None,
            volume_24h: None,
            market_cap: None,
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Active position state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivePosition {
    /// Token mint address
    pub token_mint: String,
    /// Token symbol
    pub token_symbol: String,
    /// Entry price in USDC
    pub entry_price: f64,
    /// Position size in token base units
    pub size: u64,
    /// USDC value at entry
    pub entry_value_usdc: f64,
    /// Entry timestamp
    pub entry_timestamp: u64,
    /// Entry z-score from OU process
    pub entry_z_score: f64,
    /// OU parameters at entry
    pub ou_params: Option<OUParams>,
    /// High watermark price for trailing TP
    #[serde(default)]
    pub high_watermark_price: f64,
    /// Whether trailing TP has been activated
    #[serde(default)]
    pub trailing_activated: bool,
    /// Trailing activation percentage (from config)
    #[serde(default = "default_trailing_activation")]
    pub trailing_activation_pct: f64,
    /// Trailing stop percentage (from config)
    #[serde(default = "default_trailing_stop")]
    pub trailing_stop_pct: f64,
    /// Entry ADX value (for momentum strategy)
    #[serde(default)]
    pub entry_adx: Option<f64>,
    /// Whether this is a momentum trade (vs mean reversion)
    #[serde(default)]
    pub is_momentum_trade: bool,
}

fn default_trailing_activation() -> f64 {
    10.0
}

fn default_trailing_stop() -> f64 {
    5.0
}

impl ActivePosition {
    pub fn new(
        token_mint: String,
        token_symbol: String,
        entry_price: f64,
        size: u64,
        entry_value_usdc: f64,
        entry_z_score: f64,
        ou_params: Option<OUParams>,
    ) -> Self {
        Self {
            token_mint,
            token_symbol,
            entry_price,
            size,
            entry_value_usdc,
            entry_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            entry_z_score,
            ou_params,
            high_watermark_price: entry_price,
            trailing_activated: false,
            trailing_activation_pct: default_trailing_activation(),
            trailing_stop_pct: default_trailing_stop(),
            entry_adx: None,
            is_momentum_trade: false,
        }
    }

    /// Create a new momentum position with ADX and trailing TP config
    pub fn new_momentum(
        token_mint: String,
        token_symbol: String,
        entry_price: f64,
        size: u64,
        entry_value_usdc: f64,
        entry_z_score: f64,
        ou_params: Option<OUParams>,
        entry_adx: f64,
        trailing_activation_pct: f64,
        trailing_stop_pct: f64,
    ) -> Self {
        Self {
            token_mint,
            token_symbol,
            entry_price,
            size,
            entry_value_usdc,
            entry_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            entry_z_score,
            ou_params,
            high_watermark_price: entry_price,
            trailing_activated: false,
            trailing_activation_pct,
            trailing_stop_pct,
            entry_adx: Some(entry_adx),
            is_momentum_trade: true,
        }
    }

    /// Calculate current PnL percentage given current price
    pub fn pnl_pct(&self, current_price: f64) -> f64 {
        if self.entry_price == 0.0 {
            return 0.0;
        }
        (current_price - self.entry_price) / self.entry_price * 100.0
    }

    /// Calculate position age in seconds
    pub fn age_seconds(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now.saturating_sub(self.entry_timestamp)
    }

    /// Update high watermark price and check trailing activation
    /// Returns true if the watermark was updated
    pub fn update_price(&mut self, current_price: f64) -> bool {
        let pnl_pct = self.pnl_pct(current_price);

        // Check if trailing TP should activate
        if !self.trailing_activated && pnl_pct >= self.trailing_activation_pct {
            self.trailing_activated = true;
            tracing::info!(
                "Trailing TP activated for {} at {:.2}% profit",
                self.token_symbol,
                pnl_pct
            );
        }

        // Update high watermark if price is higher
        if current_price > self.high_watermark_price {
            self.high_watermark_price = current_price;
            return true;
        }

        false
    }

    /// Calculate drawdown from high watermark
    pub fn drawdown_from_high(&self, current_price: f64) -> f64 {
        if self.high_watermark_price == 0.0 {
            return 0.0;
        }
        (self.high_watermark_price - current_price) / self.high_watermark_price * 100.0
    }

    /// Check if trailing stop has been triggered
    pub fn trailing_stop_triggered(&self, current_price: f64) -> bool {
        if !self.trailing_activated {
            return false;
        }
        self.drawdown_from_high(current_price) >= self.trailing_stop_pct
    }
}

/// Persisted state for crash recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    /// Active position if any
    pub active_position: Option<ActivePosition>,
    /// Last update timestamp
    pub last_updated: u64,
    /// Wallet address
    pub wallet: String,
}

impl PersistedState {
    pub fn new(wallet: String) -> Self {
        Self {
            active_position: None,
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            wallet,
        }
    }

    pub fn with_position(mut self, position: ActivePosition) -> Self {
        self.active_position = Some(position);
        self.last_updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self
    }

    pub fn clear_position(mut self) -> Self {
        self.active_position = None;
        self.last_updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self
    }

    /// Load from file
    pub fn load(path: &Path) -> Result<Option<Self>, MemeOrchestratorError> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        let state: Self = serde_json::from_str(&content)
            .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        Ok(Some(state))
    }

    /// Save to file
    pub fn save(&self, path: &Path) -> Result<(), MemeOrchestratorError> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        std::fs::write(path, content)
            .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        Ok(())
    }

    /// Delete file
    pub fn delete(path: &Path) -> Result<(), MemeOrchestratorError> {
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| MemeOrchestratorError::PersistenceError(e.to_string()))?;
        }
        Ok(())
    }
}

/// Configuration for the meme orchestrator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemeOrchestratorConfig {
    /// Enable meme coin trading
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// OU process lookback period (number of samples)
    pub ou_lookback: usize,
    /// Time step between samples in minutes
    pub ou_dt_minutes: f64,
    /// Z-score threshold for entry (e.g., -3.5 for oversold)
    pub z_entry_threshold: f64,
    /// Z-score threshold for exit (e.g., 0.0 for mean reversion)
    pub z_exit_threshold: f64,
    /// Stop loss percentage
    pub stop_loss_pct: f64,
    /// Take profit percentage
    pub take_profit_pct: f64,
    /// Maximum position age in hours before forced exit
    pub max_position_hours: f64,
    /// Trade size in USDC
    pub trade_size_usdc: f64,
    /// Slippage tolerance in basis points
    pub slippage_bps: u16,
    /// Poll interval in seconds
    pub poll_interval_secs: u64,
    /// Priority fee in lamports
    pub priority_fee_lamports: u64,
    /// Data directory for persistence
    pub data_dir: PathBuf,
    /// Enable paper trading mode
    pub paper_mode: bool,
    /// Minimum confidence for OU parameters
    pub min_ou_confidence: f64,
    /// Minimum half-life in minutes for valid OU process
    pub min_half_life_minutes: f64,
    /// Maximum half-life in minutes
    pub max_half_life_minutes: f64,

    // =========================================================================
    // Momentum Strategy Parameters
    // =========================================================================
    /// Enable momentum-based trading (if false, uses mean reversion)
    #[serde(default)]
    pub momentum_enabled: bool,
    /// Z-score threshold for momentum entry (positive = with trend)
    #[serde(default = "default_momentum_z")]
    pub momentum_z_threshold: f64,
    /// ADX threshold for entry confirmation
    #[serde(default = "default_adx_entry")]
    pub momentum_adx_entry_threshold: f64,
    /// ADX threshold below which trend is dying
    #[serde(default = "default_adx_exit")]
    pub momentum_adx_exit_threshold: f64,
    /// Hours after which to check for ADX decay
    #[serde(default = "default_decay_hours")]
    pub momentum_decay_hours: f64,

    // =========================================================================
    // Trailing Take Profit Parameters
    // =========================================================================
    /// Enable trailing take profit
    #[serde(default)]
    pub use_trailing_tp: bool,
    /// Profit percentage at which trailing stop activates
    #[serde(default = "default_trailing_activation")]
    pub trailing_activation_pct: f64,
    /// Percentage drawdown from high watermark that triggers exit
    #[serde(default = "default_trailing_stop")]
    pub trailing_stop_pct: f64,

    // =========================================================================
    // Timing & Risk Parameters
    // =========================================================================
    /// Cooldown between trades in seconds
    #[serde(default = "default_cooldown_seconds")]
    pub cooldown_seconds: u64,
    /// Maximum trades per day
    #[serde(default = "default_max_daily_trades")]
    pub max_daily_trades: u32,
}

fn default_momentum_z() -> f64 { 1.5 }
fn default_adx_entry() -> f64 { 25.0 }
fn default_adx_exit() -> f64 { 20.0 }
fn default_decay_hours() -> f64 { 4.0 }
fn default_cooldown_seconds() -> u64 { 300 } // 5 minutes
fn default_max_daily_trades() -> u32 { 10 }

fn default_enabled() -> bool {
    false
}

impl Default for MemeOrchestratorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            ou_lookback: 100,
            ou_dt_minutes: 1.0,
            z_entry_threshold: -3.5,
            z_exit_threshold: 0.0,
            stop_loss_pct: 8.0, // Tighter for momentum
            take_profit_pct: 15.0,
            max_position_hours: 6.0, // Extended for momentum
            trade_size_usdc: 50.0,
            slippage_bps: 100, // 1%
            poll_interval_secs: 60,
            priority_fee_lamports: 10_000,
            data_dir: PathBuf::from("data"),
            paper_mode: true,
            min_ou_confidence: 0.3,
            min_half_life_minutes: 5.0,
            max_half_life_minutes: 120.0,
            // Momentum defaults
            momentum_enabled: false,
            momentum_z_threshold: default_momentum_z(),
            momentum_adx_entry_threshold: default_adx_entry(),
            momentum_adx_exit_threshold: default_adx_exit(),
            momentum_decay_hours: default_decay_hours(),
            // Trailing TP defaults
            use_trailing_tp: false,
            trailing_activation_pct: default_trailing_activation(),
            trailing_stop_pct: default_trailing_stop(),
            // Timing & Risk defaults
            cooldown_seconds: default_cooldown_seconds(),
            max_daily_trades: default_max_daily_trades(),
        }
    }
}

impl MemeOrchestratorConfig {
    /// Validate configuration parameters
    pub fn validate(&self) -> Result<(), MemeOrchestratorError> {
        if self.ou_lookback == 0 {
            return Err(MemeOrchestratorError::ConfigError(
                "ou_lookback must be > 0".to_string(),
            ));
        }

        if self.ou_dt_minutes <= 0.0 {
            return Err(MemeOrchestratorError::ConfigError(
                "ou_dt_minutes must be > 0".to_string(),
            ));
        }

        if self.stop_loss_pct <= 0.0 || self.stop_loss_pct > 100.0 {
            return Err(MemeOrchestratorError::ConfigError(
                "stop_loss_pct must be between 0 and 100".to_string(),
            ));
        }

        if self.take_profit_pct <= 0.0 || self.take_profit_pct > 100.0 {
            return Err(MemeOrchestratorError::ConfigError(
                "take_profit_pct must be between 0 and 100".to_string(),
            ));
        }

        if self.trade_size_usdc <= 0.0 {
            return Err(MemeOrchestratorError::ConfigError(
                "trade_size_usdc must be > 0".to_string(),
            ));
        }

        if self.min_ou_confidence < 0.0 || self.min_ou_confidence > 1.0 {
            return Err(MemeOrchestratorError::ConfigError(
                "min_ou_confidence must be between 0 and 1".to_string(),
            ));
        }

        if self.min_half_life_minutes >= self.max_half_life_minutes {
            return Err(MemeOrchestratorError::ConfigError(
                "min_half_life_minutes must be < max_half_life_minutes".to_string(),
            ));
        }

        Ok(())
    }
}

/// Per-token tracking state
#[derive(Debug)]
pub struct TokenTracker {
    /// Token info
    pub info: TokenInfo,
    /// OU process estimator for this token
    pub ou_process: OUProcess,
    /// Last price update time
    pub last_price_time: Option<Instant>,
    /// Price history for additional analysis
    pub price_history: Vec<f64>,
    /// Maximum history to keep
    pub max_history: usize,
    /// Momentum ADX detector for trend confirmation
    pub momentum_adx: MomentumAdxDetector,
    /// Candle builder for ADX (5-minute candles)
    pub candle_builder: CandleBuilder,
    /// Previous ADX value for decay detection
    pub prev_adx_value: Option<f64>,
}

impl TokenTracker {
    pub fn new(info: TokenInfo, ou_lookback: usize, ou_dt_minutes: f64) -> Self {
        Self {
            info,
            ou_process: OUProcess::new(ou_lookback, ou_dt_minutes),
            last_price_time: None,
            price_history: Vec::with_capacity(ou_lookback),
            max_history: ou_lookback * 2,
            // Momentum ADX detector (meme-optimized: faster period)
            momentum_adx: MomentumAdxDetector::meme_optimized(),
            // 5-minute candles for ADX - reduces noise
            candle_builder: CandleBuilder::five_minute(),
            prev_adx_value: None,
        }
    }

    /// Update with new price, returns OU signal
    pub fn update_price(&mut self, price: f64) -> OUSignal {
        self.info.price_usdc = Some(price);
        self.info.last_updated = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_price_time = Some(Instant::now());

        // Maintain price history
        self.price_history.push(price);
        while self.price_history.len() > self.max_history {
            self.price_history.remove(0);
        }

        // Feed price to candle builder (5-min candles for ADX)
        if let Some(candle) = self.candle_builder.update(price) {
            // Store previous ADX for decay detection
            if self.momentum_adx.is_valid() {
                self.prev_adx_value = Some(self.momentum_adx.adx());
            }
            // Update ADX with completed candle
            self.momentum_adx.update(&candle);
        }

        // Update OU process
        self.ou_process.update(price)
    }

    /// Check if this token has valid OU parameters for trading
    pub fn is_tradeable(&self, config: &MemeOrchestratorConfig) -> bool {
        if let Some(params) = self.ou_process.params() {
            if !params.is_valid() {
                return false;
            }
            if params.confidence < config.min_ou_confidence {
                return false;
            }
            if let Some(half_life_min) = self.ou_process.half_life_minutes() {
                if half_life_min < config.min_half_life_minutes
                    || half_life_min > config.max_half_life_minutes
                {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
}

/// Meme coin trading orchestrator
pub struct MemeOrchestrator {
    /// Configuration
    config: MemeOrchestratorConfig,
    /// Jupiter client for swaps
    jupiter: JupiterClient,
    /// Solana RPC client
    solana: SolanaClient,
    /// Wallet manager
    wallet: WalletManager,
    /// Per-token tracking
    tokens: Arc<RwLock<HashMap<String, TokenTracker>>>,
    /// Current active position (only one at a time)
    active_position: Arc<RwLock<Option<ActivePosition>>>,
    /// Trade lock to prevent race conditions
    trade_lock: Arc<Mutex<()>>,
    /// Balance guard for security
    balance_guard: Arc<RwLock<BalanceGuard>>,
    /// Running state
    is_running: Arc<RwLock<bool>>,
    /// Shutdown signal
    shutdown_requested: Arc<RwLock<bool>>,
    /// Rug detector for token safety analysis
    rug_detector: Arc<RwLock<RugDetector>>,
    /// Liquidity guard for LP monitoring
    liquidity_guard: Arc<RwLock<LiquidityGuard>>,
    /// Honeypot detector for transfer restriction checks
    honeypot_detector: Arc<dyn HoneypotDetector>,
}

impl MemeOrchestrator {
    /// Create a new meme orchestrator
    pub fn new(
        config: MemeOrchestratorConfig,
        jupiter: JupiterClient,
        solana: SolanaClient,
        wallet: WalletManager,
    ) -> Result<Self, MemeOrchestratorError> {
        let balance_guard = BalanceGuard::new(wallet.pubkey());

        // Initialize safety modules with graduated token settings (stricter than pump.fun)
        let rug_detector = RugDetector::new();
        let liquidity_guard = LiquidityGuard::graduated();

        // Production honeypot detector with Token-2022 extension analysis and sell simulation
        // Replaces StubHoneypotDetector which always returned safe
        let honeypot_detector: Arc<dyn HoneypotDetector> = Arc::new(
            SolanaHoneypotDetector::new(solana.clone(), jupiter.clone()),
        );

        Ok(Self {
            config,
            jupiter,
            solana,
            wallet: wallet.clone(),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            active_position: Arc::new(RwLock::new(None)),
            trade_lock: Arc::new(Mutex::new(())),
            balance_guard: Arc::new(RwLock::new(balance_guard)),
            is_running: Arc::new(RwLock::new(false)),
            shutdown_requested: Arc::new(RwLock::new(false)),
            rug_detector: Arc::new(RwLock::new(rug_detector)),
            liquidity_guard: Arc::new(RwLock::new(liquidity_guard)),
            honeypot_detector,
        })
    }

    /// Load persisted state on startup
    pub async fn load_persisted_state(&self) -> Result<(), MemeOrchestratorError> {
        let path = self.config.data_dir.join(POSITION_FILE);

        if let Some(state) = PersistedState::load(&path)? {
            if state.wallet != self.wallet.pubkey().to_string() {
                tracing::warn!(
                    "Persisted state wallet mismatch: {} vs {}",
                    state.wallet,
                    self.wallet.pubkey()
                );
                return Ok(());
            }

            if let Some(position) = state.active_position {
                tracing::info!(
                    "Recovered position: {} {} at ${:.6}",
                    position.token_symbol,
                    position.size,
                    position.entry_price
                );
                *self.active_position.write().await = Some(position);
            }
        }

        Ok(())
    }

    /// Save current state for crash recovery
    pub async fn persist_state(&self) -> Result<(), MemeOrchestratorError> {
        let position = self.active_position.read().await.clone();
        let state = PersistedState::new(self.wallet.pubkey().to_string());
        let state = if let Some(pos) = position {
            state.with_position(pos)
        } else {
            state
        };

        let path = self.config.data_dir.join(POSITION_FILE);
        state.save(&path)?;

        Ok(())
    }

    /// Add a token to track
    pub async fn add_token(&self, info: TokenInfo) {
        let tracker = TokenTracker::new(
            info.clone(),
            self.config.ou_lookback,
            self.config.ou_dt_minutes,
        );
        self.tokens.write().await.insert(info.mint.clone(), tracker);
        tracing::info!("Added token: {} ({})", info.symbol, info.mint);
    }

    /// Remove a token from tracking
    pub async fn remove_token(&self, mint: &str) {
        self.tokens.write().await.remove(mint);
        tracing::info!("Removed token: {}", mint);
    }

    /// Update price for a specific token
    pub async fn update_token_price(
        &self,
        mint: &str,
        price: f64,
    ) -> Result<OUSignal, MemeOrchestratorError> {
        let mut tokens = self.tokens.write().await;
        let tracker = tokens
            .get_mut(mint)
            .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;

        let signal = tracker.update_price(price);
        Ok(signal)
    }

    /// Get current OU parameters for a token
    pub async fn get_token_ou_params(&self, mint: &str) -> Option<OUParams> {
        let tokens = self.tokens.read().await;
        tokens
            .get(mint)
            .and_then(|t| t.ou_process.params().cloned())
    }

    /// Check if a token is ready for trading
    pub async fn is_token_tradeable(&self, mint: &str) -> bool {
        let tokens = self.tokens.read().await;
        if let Some(tracker) = tokens.get(mint) {
            tracker.is_tradeable(&self.config)
        } else {
            false
        }
    }

    /// Fetch price for a token using Jupiter quote
    pub async fn fetch_token_price(&self, mint: &str) -> Result<f64, MemeOrchestratorError> {
        // Quote 1 unit of token to USDC
        let tracker = {
            let tokens = self.tokens.read().await;
            tokens
                .get(mint)
                .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?
                .info
                .clone()
        };

        let amount = 10u64.pow(tracker.decimals as u32); // 1 token

        let quote_request = QuoteRequest::new(
            mint.to_string(),
            USDC_MINT.to_string(),
            amount,
            self.config.slippage_bps,
        );

        let quote = self
            .jupiter
            .get_quote(&quote_request)
            .await
            .map_err(|e| MemeOrchestratorError::MarketDataError(e.to_string()))?;

        // USDC has 6 decimals
        let price = quote.output_amount() as f64 / 1_000_000.0;

        Ok(price)
    }

    /// Check meme token safety before entry
    ///
    /// Performs safety checks using:
    /// - RugDetector: Analyzes holder distribution, authorities, metadata
    /// - LiquidityGuard: Monitors liquidity levels for rug risk
    /// - HoneypotDetector: Checks for transfer restrictions (sell blocked)
    ///
    /// Returns true if the token passes safety checks.
    pub async fn check_meme_safety(&self, mint: &str) -> Result<bool, MemeOrchestratorError> {
        // Get token info for logging
        let symbol = {
            let tokens = self.tokens.read().await;
            tokens.get(mint).map(|t| t.info.symbol.clone()).unwrap_or_else(|| mint[..8].to_string())
        };

        // =========================================================================
        // 1. RUG DETECTOR CHECK
        // =========================================================================
        // Note: Full rug detection requires fetching token metadata, holder data,
        // and liquidity info from chain. For now, we check cached reports or
        // use analyze_with_partial_data. In production, expand this to fetch
        // real data from Birdeye/Helius APIs.
        let rug_detector = self.rug_detector.read().await;

        // Check if we have a cached safety report
        if let Some(cached_report) = rug_detector.get_cached(mint) {
            if cached_report.risk_level.should_block() {
                tracing::warn!(
                    "Rug detector blocked {}: {:?} risk - {:?}",
                    symbol,
                    cached_report.risk_level,
                    cached_report.warnings
                );
                return Ok(false);
            }
            if cached_report.risk_level.should_warn() {
                tracing::warn!(
                    "Rug detector warning for {}: {:?} risk - {:?}",
                    symbol,
                    cached_report.risk_level,
                    cached_report.warnings
                );
                // Continue but with warning logged
            }
        }
        drop(rug_detector);

        // =========================================================================
        // 2. LIQUIDITY GUARD CHECK
        // =========================================================================
        // Check minimum liquidity threshold
        let tokens = self.tokens.read().await;
        if let Some(tracker) = tokens.get(mint) {
            // Use market cap as proxy for liquidity if available
            if let Some(market_cap) = tracker.info.market_cap {
                let liquidity_guard = self.liquidity_guard.read().await;
                let min_liquidity = liquidity_guard.config().min_liquidity_usd;

                // Market cap should be at least 10x the minimum liquidity for safety
                if market_cap < min_liquidity * 10.0 {
                    tracing::warn!(
                        "Liquidity guard blocked {}: market cap ${:.0} < ${:.0} min (10x liquidity)",
                        symbol,
                        market_cap,
                        min_liquidity * 10.0
                    );
                    return Ok(false);
                }
            }
        }
        drop(tokens);

        // =========================================================================
        // 3. HONEYPOT DETECTOR CHECK
        // =========================================================================
        // Check if token can be sold (not a honeypot)
        // Note: StubHoneypotDetector always returns safe - replace with real
        // implementation that checks Token-2022 extensions, transfer hooks, etc.
        let mint_pubkey = match mint.parse::<solana_sdk::pubkey::Pubkey>() {
            Ok(pk) => pk,
            Err(_) => {
                tracing::warn!("Invalid mint address for honeypot check: {}", mint);
                return Ok(false);
            }
        };

        match self.honeypot_detector.can_sell(&mint_pubkey).await {
            Ok(can_sell) => {
                if !can_sell {
                    tracing::warn!(
                        "Honeypot detector blocked {}: token cannot be sold",
                        symbol
                    );
                    return Ok(false);
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Honeypot check failed for {}: {:?} - proceeding with caution",
                    symbol,
                    e
                );
                // Don't block on honeypot check errors, but log warning
            }
        }

        // Optional: Full honeypot analysis for additional checks
        match self.honeypot_detector.analyze(&mint_pubkey).await {
            Ok(analysis) => {
                if analysis.risk_level.should_block() {
                    tracing::warn!(
                        "Honeypot analysis blocked {}: {:?} risk - {:?}",
                        symbol,
                        analysis.risk_level,
                        analysis.issues
                    );
                    return Ok(false);
                }
                if analysis.risk_level.should_warn() {
                    tracing::warn!(
                        "Honeypot warning for {}: {:?} - {:?}",
                        symbol,
                        analysis.risk_level,
                        analysis.issues
                    );
                }
            }
            Err(e) => {
                tracing::debug!("Honeypot analysis unavailable for {}: {:?}", symbol, e);
            }
        }

        tracing::debug!("Safety checks passed for {}", symbol);
        Ok(true)
    }

    /// Check for entry signal on a specific token
    ///
    /// Entry logic depends on momentum_enabled config:
    /// - Momentum mode: z > +threshold AND ADX > entry_threshold (BullishMomentum)
    /// - Mean reversion mode: z < -threshold (oversold)
    ///
    /// NOTE: LONG-ONLY for meme tokens - Jupiter doesn't support shorting
    pub async fn check_entry_signal(&self, mint: &str) -> Result<bool, MemeOrchestratorError> {
        // Can only enter if no position open
        if self.active_position.read().await.is_some() {
            return Ok(false);
        }

        // =========================================================================
        // MEME SAFETY CHECK (before any technical analysis)
        // =========================================================================
        if !self.check_meme_safety(mint).await? {
            return Ok(false);
        }

        let tokens = self.tokens.read().await;
        let tracker = tokens
            .get(mint)
            .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;

        // Check if tradeable
        if !tracker.is_tradeable(&self.config) {
            return Ok(false);
        }

        // Get z-score from OU process
        let z_score = match tracker.ou_process.current_z_score() {
            Some(z) => z,
            None => return Ok(false),
        };

        if self.config.momentum_enabled {
            // =========================================================================
            // MOMENTUM ENTRY: z WITH trend + ADX confirmation
            // NOTE: LONG-ONLY for meme tokens (Jupiter doesn't support shorting)
            // =========================================================================
            if z_score > self.config.momentum_z_threshold {
                // Check ADX for trend confirmation
                if let Some(signal) = tracker.momentum_adx.check_entry_signal() {
                    if let MomentumSignal::BullishMomentum { adx, plus_di, minus_di } = signal {
                        tracing::info!(
                            "Momentum LONG signal for {}: z={:.2} > {:.2}, ADX={:.1} (+DI={:.1}, -DI={:.1})",
                            tracker.info.symbol,
                            z_score,
                            self.config.momentum_z_threshold,
                            adx,
                            plus_di,
                            minus_di
                        );
                        return Ok(true);
                    }
                    // BearishMomentum intentionally ignored - no shorting on meme coins
                }
            }
        } else {
            // =========================================================================
            // MEAN REVERSION ENTRY: z < threshold (oversold)
            // =========================================================================
            if z_score < self.config.z_entry_threshold {
                tracing::info!(
                    "Mean reversion entry signal for {}: z={:.2} < {:.2}",
                    tracker.info.symbol,
                    z_score,
                    self.config.z_entry_threshold
                );
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Check for exit signal on current position
    ///
    /// Exit priority (first match wins):
    /// 1. Stop loss (unchanged)
    /// 2. Trailing stop (if activated)
    /// 3. ADX trend dying (momentum mode: adx < exit_threshold)
    /// 4. Momentum decay (after decay_hours: ADX falling)
    /// 5. Fixed take profit (fallback if trailing not used)
    /// 6. Time stop (max_position_hours)
    /// 7. Z-score reversal (mean reversion mode)
    pub async fn check_exit_signal(&self) -> Result<bool, MemeOrchestratorError> {
        let position = match self.active_position.read().await.clone() {
            Some(p) => p,
            None => return Ok(false),
        };

        let tokens = self.tokens.read().await;
        let tracker = tokens.get(&position.token_mint);

        if let Some(tracker) = tracker {
            let current_price = tracker.info.price_usdc.unwrap_or(position.entry_price);
            let pnl_pct = position.pnl_pct(current_price);
            let age_hours = position.age_seconds() as f64 / 3600.0;

            // =========================================================================
            // 1. STOP LOSS (always first priority)
            // =========================================================================
            if pnl_pct <= -self.config.stop_loss_pct {
                tracing::warn!(
                    "Stop loss triggered for {}: {:.2}%",
                    position.token_symbol,
                    pnl_pct
                );
                return Ok(true);
            }

            // =========================================================================
            // 2. TRAILING STOP (if enabled and activated)
            // =========================================================================
            if self.config.use_trailing_tp && position.trailing_activated {
                if position.trailing_stop_triggered(current_price) {
                    let drawdown = position.drawdown_from_high(current_price);
                    tracing::info!(
                        "Trailing stop triggered for {}: {:.2}% drawdown from high (price: ${:.4}, high: ${:.4})",
                        position.token_symbol,
                        drawdown,
                        current_price,
                        position.high_watermark_price
                    );
                    return Ok(true);
                }
            }

            // =========================================================================
            // 3. ADX TREND DYING (momentum mode only)
            // =========================================================================
            if self.config.momentum_enabled && position.is_momentum_trade {
                let adx = tracker.momentum_adx.adx();
                if tracker.momentum_adx.is_valid() && adx < self.config.momentum_adx_exit_threshold {
                    tracing::info!(
                        "ADX trend dying for {}: ADX={:.1} < {:.1}",
                        position.token_symbol,
                        adx,
                        self.config.momentum_adx_exit_threshold
                    );
                    return Ok(true);
                }
            }

            // =========================================================================
            // 4. MOMENTUM DECAY (after decay_hours, exit if ADX falling)
            // =========================================================================
            if self.config.momentum_enabled && position.is_momentum_trade {
                if age_hours >= self.config.momentum_decay_hours {
                    // Check if ADX is falling (momentum decay)
                    if let Some(prev_adx) = tracker.prev_adx_value {
                        let current_adx = tracker.momentum_adx.adx();
                        let adx_falling = current_adx < prev_adx;
                        // Exit if ADX is falling but still above exit threshold (decay, not death)
                        if adx_falling && current_adx > self.config.momentum_adx_exit_threshold {
                            tracing::info!(
                                "Momentum decay exit for {}: ADX falling {:.1} -> {:.1} after {:.1}h",
                                position.token_symbol,
                                prev_adx,
                                current_adx,
                                age_hours
                            );
                            return Ok(true);
                        }
                    }
                }
            }

            // =========================================================================
            // 5. FIXED TAKE PROFIT (fallback if trailing TP not used)
            // =========================================================================
            if !self.config.use_trailing_tp {
                if pnl_pct >= self.config.take_profit_pct {
                    tracing::info!(
                        "Take profit triggered for {}: {:.2}%",
                        position.token_symbol,
                        pnl_pct
                    );
                    return Ok(true);
                }
            }

            // =========================================================================
            // 6. TIME STOP (max position hours)
            // =========================================================================
            if age_hours >= self.config.max_position_hours {
                tracing::info!(
                    "Time stop triggered for {}: {:.1}h",
                    position.token_symbol,
                    age_hours
                );
                return Ok(true);
            }

            // =========================================================================
            // 7. Z-SCORE REVERSAL (mean reversion mode only)
            // =========================================================================
            if !self.config.momentum_enabled {
                if let Some(z_score) = tracker.ou_process.current_z_score() {
                    if z_score >= self.config.z_exit_threshold {
                        tracing::info!(
                            "Mean reversion exit for {}: z={:.2}",
                            position.token_symbol,
                            z_score
                        );
                        return Ok(true);
                    }
                }
            }
        } else {
            // Token no longer tracked - force exit
            tracing::warn!(
                "Token {} no longer tracked, forcing exit",
                position.token_symbol
            );
            return Ok(true);
        }

        Ok(false)
    }

    /// Execute entry trade (USDC -> Token)
    pub async fn execute_entry(
        &self,
        mint: &str,
    ) -> Result<ActivePosition, MemeOrchestratorError> {
        // Acquire trade lock
        let _lock = self
            .trade_lock
            .try_lock()
            .map_err(|_| MemeOrchestratorError::TradeLockFailed)?;

        // Double-check no position
        if self.active_position.read().await.is_some() {
            return Err(MemeOrchestratorError::PositionAlreadyOpen(
                "Already have position".to_string(),
            ));
        }

        // Check trading not halted
        if self.balance_guard.read().await.is_halted() {
            return Err(MemeOrchestratorError::TradingHalted(
                "Balance guard halted".to_string(),
            ));
        }

        let (tracker_info, z_score, ou_params, entry_adx) = {
            let tokens = self.tokens.read().await;
            let tracker = tokens
                .get(mint)
                .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;
            (
                tracker.info.clone(),
                tracker.ou_process.current_z_score().unwrap_or(0.0),
                tracker.ou_process.params().cloned(),
                if tracker.momentum_adx.is_valid() {
                    Some(tracker.momentum_adx.adx())
                } else {
                    None
                },
            )
        };

        if self.config.paper_mode {
            // Paper trade - simulate entry
            let price = tracker_info.price_usdc.unwrap_or(0.0);
            let size = ((self.config.trade_size_usdc / price)
                * 10f64.powi(tracker_info.decimals as i32)) as u64;

            let position = if self.config.momentum_enabled {
                // Momentum trade with ADX and trailing TP config
                ActivePosition::new_momentum(
                    mint.to_string(),
                    tracker_info.symbol.clone(),
                    price,
                    size,
                    self.config.trade_size_usdc,
                    z_score,
                    ou_params,
                    entry_adx.unwrap_or(0.0),
                    self.config.trailing_activation_pct,
                    self.config.trailing_stop_pct,
                )
            } else {
                // Mean reversion trade
                ActivePosition::new(
                    mint.to_string(),
                    tracker_info.symbol.clone(),
                    price,
                    size,
                    self.config.trade_size_usdc,
                    z_score,
                    ou_params,
                )
            };

            if self.config.momentum_enabled {
                tracing::info!(
                    "PAPER MOMENTUM ENTRY: {} {} at ${:.8} (z={:.2}, ADX={:.1})",
                    tracker_info.symbol,
                    size,
                    price,
                    z_score,
                    entry_adx.unwrap_or(0.0)
                );
            } else {
                tracing::info!(
                    "PAPER ENTRY: {} {} at ${:.8} (z={:.2})",
                    tracker_info.symbol,
                    size,
                    price,
                    z_score
                );
            }

            *self.active_position.write().await = Some(position.clone());
            self.persist_state().await?;

            return Ok(position);
        }

        // Real trade - USDC -> Token
        let usdc_amount = (self.config.trade_size_usdc * 1_000_000.0) as u64;

        // Get quote
        let quote_request = QuoteRequest::new(
            USDC_MINT.to_string(),
            mint.to_string(),
            usdc_amount,
            self.config.slippage_bps,
        );

        let quote = self
            .jupiter
            .get_quote(&quote_request)
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        // Check price impact
        let price_impact = quote.price_impact();
        if price_impact > MAX_PRICE_IMPACT_PCT {
            return Err(MemeOrchestratorError::PriceImpactTooHigh(price_impact));
        }

        // Capture pre-trade balance
        let pre_balance = self
            .solana
            .get_balance(&self.wallet.pubkey().to_string())
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;
        self.balance_guard
            .write()
            .await
            .capture_pre_trade(pre_balance);

        // Build and execute swap
        let quote_json = serde_json::to_value(&quote)
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        let swap_request = SwapRequest::new(self.wallet.public_key(), quote_json)
            .with_priority_fee(self.config.priority_fee_lamports);

        let swap_response = self
            .jupiter
            .get_swap_transaction(&swap_request)
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        // TODO: Wave 3 - Sign and submit transaction via Jito or direct RPC
        // For now, log the intent
        tracing::info!(
            "Executing entry swap: {} USDC -> {} (impact: {:.4}%)",
            self.config.trade_size_usdc,
            tracker_info.symbol,
            price_impact
        );

        // Calculate position
        let output_amount = quote.output_amount();
        let price = self.config.trade_size_usdc
            / (output_amount as f64 / 10f64.powi(tracker_info.decimals as i32));

        let position = if self.config.momentum_enabled {
            // Momentum trade with ADX and trailing TP config
            ActivePosition::new_momentum(
                mint.to_string(),
                tracker_info.symbol.clone(),
                price,
                output_amount,
                self.config.trade_size_usdc,
                z_score,
                ou_params,
                entry_adx.unwrap_or(0.0),
                self.config.trailing_activation_pct,
                self.config.trailing_stop_pct,
            )
        } else {
            // Mean reversion trade
            ActivePosition::new(
                mint.to_string(),
                tracker_info.symbol.clone(),
                price,
                output_amount,
                self.config.trade_size_usdc,
                z_score,
                ou_params,
            )
        };

        // Validate post-trade balance (fees only, we're spending USDC not SOL)
        let post_balance = self
            .solana
            .get_balance(&self.wallet.pubkey().to_string())
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        let expected_delta = ExpectedDelta::custom(
            -(swap_response.prioritization_fee_lamports as i64) - 5000,
            format!("Entry: USDC -> {}", tracker_info.symbol),
        );

        if let Err(e) = self
            .balance_guard
            .write()
            .await
            .validate_post_trade(post_balance, &expected_delta)
        {
            tracing::error!("Balance guard violation on entry: {:?}", e);
        }

        *self.active_position.write().await = Some(position.clone());
        self.persist_state().await?;

        Ok(position)
    }

    /// Execute exit trade (Token -> USDC)
    pub async fn execute_exit(&self) -> Result<f64, MemeOrchestratorError> {
        // Acquire trade lock
        let _lock = self
            .trade_lock
            .try_lock()
            .map_err(|_| MemeOrchestratorError::TradeLockFailed)?;

        let position = self
            .active_position
            .read()
            .await
            .clone()
            .ok_or(MemeOrchestratorError::NoPositionOpen)?;

        if self.config.paper_mode {
            // Paper trade - simulate exit
            let current_price = {
                let tokens = self.tokens.read().await;
                tokens
                    .get(&position.token_mint)
                    .and_then(|t| t.info.price_usdc)
                    .unwrap_or(position.entry_price)
            };

            let pnl_pct = position.pnl_pct(current_price);

            tracing::info!(
                "PAPER EXIT: {} at ${:.8} (PnL: {:.2}%)",
                position.token_symbol,
                current_price,
                pnl_pct
            );

            *self.active_position.write().await = None;
            self.persist_state().await?;

            return Ok(pnl_pct);
        }

        // Check trading not halted
        if self.balance_guard.read().await.is_halted() {
            return Err(MemeOrchestratorError::TradingHalted(
                "Balance guard halted".to_string(),
            ));
        }

        // Real trade - Token -> USDC
        let quote_request = QuoteRequest::new(
            position.token_mint.clone(),
            USDC_MINT.to_string(),
            position.size,
            self.config.slippage_bps,
        );

        let quote = self
            .jupiter
            .get_quote(&quote_request)
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        // Check price impact
        let price_impact = quote.price_impact();
        if price_impact > MAX_PRICE_IMPACT_PCT * 2.0 {
            // More lenient on exit
            tracing::warn!(
                "High price impact on exit: {:.2}%, proceeding anyway",
                price_impact
            );
        }

        // Capture pre-trade balance
        let pre_balance = self
            .solana
            .get_balance(&self.wallet.pubkey().to_string())
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;
        self.balance_guard
            .write()
            .await
            .capture_pre_trade(pre_balance);

        // Build and execute swap
        let quote_json = serde_json::to_value(&quote)
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        let swap_request = SwapRequest::new(self.wallet.public_key(), quote_json)
            .with_priority_fee(self.config.priority_fee_lamports);

        let swap_response = self
            .jupiter
            .get_swap_transaction(&swap_request)
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        let output_usdc = quote.output_amount() as f64 / 1_000_000.0;
        let pnl_pct = (output_usdc - position.entry_value_usdc) / position.entry_value_usdc * 100.0;

        // TODO: Wave 3 - Sign and submit transaction via Jito or direct RPC
        tracing::info!(
            "Executing exit swap: {} -> {:.2} USDC (PnL: {:.2}%)",
            position.token_symbol,
            output_usdc,
            pnl_pct
        );

        // Validate post-trade balance
        let post_balance = self
            .solana
            .get_balance(&self.wallet.pubkey().to_string())
            .await
            .map_err(|e| MemeOrchestratorError::ExecutionError(e.to_string()))?;

        let expected_delta = ExpectedDelta::custom(
            -(swap_response.prioritization_fee_lamports as i64) - 5000,
            format!("Exit: {} -> USDC", position.token_symbol),
        );

        if let Err(e) = self
            .balance_guard
            .write()
            .await
            .validate_post_trade(post_balance, &expected_delta)
        {
            tracing::error!("Balance guard violation on exit: {:?}", e);
        }

        *self.active_position.write().await = None;
        self.persist_state().await?;

        Ok(pnl_pct)
    }

    /// Run the main trading loop
    pub async fn run(&self) -> Result<(), MemeOrchestratorError> {
        *self.is_running.write().await = true;

        tracing::info!(
            "Starting meme orchestrator - Paper mode: {}, Poll interval: {}s",
            self.config.paper_mode,
            self.config.poll_interval_secs
        );

        // Load any persisted state
        self.load_persisted_state().await?;

        let poll_interval = Duration::from_secs(self.config.poll_interval_secs);

        while *self.is_running.read().await && !*self.shutdown_requested.read().await {
            if let Err(e) = self.tick().await {
                tracing::error!("Tick error: {}", e);
                // Continue running despite errors
            }
            tokio::time::sleep(poll_interval).await;
        }

        // Graceful shutdown - try to close position if open
        if *self.shutdown_requested.read().await {
            if let Some(position) = self.active_position.read().await.clone() {
                tracing::warn!(
                    "Shutdown requested with open position in {}, attempting exit",
                    position.token_symbol
                );
                if let Err(e) = self.execute_exit().await {
                    tracing::error!("Failed to exit on shutdown: {}", e);
                }
            }
        }

        *self.is_running.write().await = false;
        tracing::info!("Meme orchestrator stopped");

        Ok(())
    }

    /// Execute one trading cycle
    pub async fn tick(&self) -> Result<(), MemeOrchestratorError> {
        // Update prices for all tracked tokens
        let token_mints: Vec<String> = {
            let tokens = self.tokens.read().await;
            tokens.keys().cloned().collect()
        };

        for mint in &token_mints {
            match self.fetch_token_price(mint).await {
                Ok(price) => {
                    let signal = self.update_token_price(mint, price).await?;

                    // Log status
                    let tokens = self.tokens.read().await;
                    if let Some(tracker) = tokens.get(mint) {
                        let z_score = tracker.ou_process.current_z_score();

                        tracing::debug!(
                            "{} ${:.8} | z={:.2} | half_life={:.1}min | signal={:?}",
                            tracker.info.symbol,
                            price,
                            z_score.unwrap_or(0.0),
                            tracker.ou_process.half_life_minutes().unwrap_or(0.0),
                            signal
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch price for {}: {}", mint, e);
                }
            }
        }

        // Update position high watermark for trailing TP (if position open)
        if self.config.use_trailing_tp {
            if let Some(mut position) = self.active_position.write().await.take() {
                let tokens = self.tokens.read().await;
                if let Some(tracker) = tokens.get(&position.token_mint) {
                    if let Some(current_price) = tracker.info.price_usdc {
                        // Update high watermark and check for activation
                        let old_watermark = position.high_watermark_price;
                        let was_activated = position.trailing_activated;
                        position.update_price(current_price);

                        // Log trailing TP activation
                        if !was_activated && position.trailing_activated {
                            let pnl = position.pnl_pct(current_price);
                            tracing::info!(
                                "Trailing TP activated for {}: {:.2}% profit reached (threshold: {:.1}%)",
                                position.token_symbol,
                                pnl,
                                self.config.trailing_activation_pct
                            );
                        }

                        // Log new high watermark
                        if position.high_watermark_price > old_watermark && position.trailing_activated {
                            tracing::debug!(
                                "New high watermark for {}: ${:.8} (prev: ${:.8})",
                                position.token_symbol,
                                position.high_watermark_price,
                                old_watermark
                            );
                        }
                    }
                }
                drop(tokens);
                *self.active_position.write().await = Some(position);
            }
        }

        // Check for exit first (if position open)
        if self.check_exit_signal().await? {
            match self.execute_exit().await {
                Ok(pnl) => {
                    tracing::info!("Position closed with PnL: {:.2}%", pnl);
                }
                Err(e) => {
                    tracing::error!("Failed to exit position: {}", e);
                    // Will retry next tick
                }
            }
            return Ok(());
        }

        // Check for entry (if no position)
        if self.active_position.read().await.is_none() {
            for mint in &token_mints {
                if self.check_entry_signal(mint).await? {
                    match self.execute_entry(mint).await {
                        Ok(position) => {
                            tracing::info!(
                                "Entered position: {} at ${:.8}",
                                position.token_symbol,
                                position.entry_price
                            );
                            break; // Only one position at a time
                        }
                        Err(e) => {
                            tracing::error!("Failed to enter position for {}: {}", mint, e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Request graceful shutdown
    pub async fn shutdown(&self) {
        tracing::info!("Shutdown requested");
        *self.shutdown_requested.write().await = true;
    }

    /// Stop the orchestrator immediately
    pub async fn stop(&self) {
        *self.is_running.write().await = false;
        tracing::info!("Stop signal sent to orchestrator");
    }

    /// Get current status
    pub async fn status(&self) -> MemeOrchestratorStatus {
        let position = self.active_position.read().await.clone();
        let tokens = self.tokens.read().await;

        let tracked_tokens: Vec<String> = tokens
            .values()
            .map(|t| t.info.symbol.clone())
            .collect();

        let current_price = position.as_ref().and_then(|p| {
            tokens.get(&p.token_mint).and_then(|t| t.info.price_usdc)
        });

        let pnl_pct = position
            .as_ref()
            .zip(current_price)
            .map(|(p, price)| p.pnl_pct(price));

        MemeOrchestratorStatus {
            is_running: *self.is_running.blocking_read(),
            paper_mode: self.config.paper_mode,
            tracked_tokens,
            active_position: position.map(|p| p.token_symbol),
            current_pnl_pct: pnl_pct,
            balance_guard_halted: self.balance_guard.blocking_read().is_halted(),
        }
    }

    /// Check if orchestrator is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Get active position if any
    pub async fn get_position(&self) -> Option<ActivePosition> {
        self.active_position.read().await.clone()
    }
}

/// Status snapshot
#[derive(Debug, Clone)]
pub struct MemeOrchestratorStatus {
    pub is_running: bool,
    pub paper_mode: bool,
    pub tracked_tokens: Vec<String>,
    pub active_position: Option<String>,
    pub current_pnl_pct: Option<f64>,
    pub balance_guard_halted: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_token_info() -> TokenInfo {
        TokenInfo {
            mint: "TestMint123".to_string(),
            symbol: "TEST".to_string(),
            name: "Test Token".to_string(),
            decimals: 9,
            price_usdc: Some(0.001),
            volume_24h: Some(100_000.0),
            market_cap: Some(1_000_000.0),
            last_updated: 0,
        }
    }

    #[test]
    fn test_token_info_creation() {
        let info = TokenInfo::new("mint123".to_string(), "TEST".to_string(), 9);
        assert_eq!(info.mint, "mint123");
        assert_eq!(info.symbol, "TEST");
        assert_eq!(info.decimals, 9);
        assert!(info.price_usdc.is_none());
    }

    #[test]
    fn test_active_position_pnl() {
        let position = ActivePosition::new(
            "mint".to_string(),
            "TEST".to_string(),
            0.001,
            1_000_000_000,
            50.0,
            -3.5,
            None,
        );

        // 10% gain
        let pnl = position.pnl_pct(0.0011);
        assert!((pnl - 10.0).abs() < 0.01);

        // 10% loss
        let pnl = position.pnl_pct(0.0009);
        assert!((pnl - (-10.0)).abs() < 0.01);
    }

    #[test]
    fn test_config_defaults() {
        let config = MemeOrchestratorConfig::default();
        assert_eq!(config.ou_lookback, 100);
        assert_eq!(config.z_entry_threshold, -3.5);
        assert_eq!(config.z_exit_threshold, 0.0);
        assert!(config.paper_mode);
    }

    #[test]
    fn test_token_tracker_creation() {
        let info = create_test_token_info();
        let tracker = TokenTracker::new(info.clone(), 100, 1.0);
        assert_eq!(tracker.info.symbol, "TEST");
        assert!(!tracker.ou_process.is_ready());
    }

    #[test]
    fn test_token_tracker_price_update() {
        let info = create_test_token_info();
        let mut tracker = TokenTracker::new(info, 50, 1.0);

        // Update prices
        for i in 0..60 {
            tracker.update_price(0.001 + (i as f64) * 0.00001);
        }

        assert!(tracker.price_history.len() > 0);
        assert!(tracker.last_price_time.is_some());
    }

    #[test]
    fn test_persisted_state_creation() {
        let state = PersistedState::new("wallet123".to_string());
        assert!(state.active_position.is_none());
        assert_eq!(state.wallet, "wallet123");
    }

    #[test]
    fn test_persisted_state_with_position() {
        let position = ActivePosition::new(
            "mint".to_string(),
            "TEST".to_string(),
            0.001,
            1_000_000_000,
            50.0,
            -3.5,
            None,
        );

        let state = PersistedState::new("wallet123".to_string()).with_position(position);
        assert!(state.active_position.is_some());
    }

    #[test]
    fn test_persisted_state_clear_position() {
        let position = ActivePosition::new(
            "mint".to_string(),
            "TEST".to_string(),
            0.001,
            1_000_000_000,
            50.0,
            -3.5,
            None,
        );

        let state = PersistedState::new("wallet123".to_string())
            .with_position(position)
            .clear_position();
        assert!(state.active_position.is_none());
    }

    #[test]
    fn test_position_age() {
        let position = ActivePosition::new(
            "mint".to_string(),
            "TEST".to_string(),
            0.001,
            1_000_000_000,
            50.0,
            -3.5,
            None,
        );

        // Age should be very small (just created)
        assert!(position.age_seconds() < 2);
    }

    #[test]
    fn test_token_tracker_is_tradeable() {
        let config = MemeOrchestratorConfig::default();
        let info = create_test_token_info();
        let tracker = TokenTracker::new(info, 50, 1.0);

        // Not tradeable without enough data
        assert!(!tracker.is_tradeable(&config));
    }
}
