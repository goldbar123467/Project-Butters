//! Meme Coin Orchestrator
//!
//! Multi-token trading loop for meme coins using the OU-GBM strategy.
//! Key features:
//! - HashMap of OUGBMStrategy per token (when implemented by workers)
//! - Token discovery via Jupiter API
//! - Single active position at a time
//! - Always settles to USDC
//! - Graceful shutdown handling
//! - Trade lock for race condition prevention
//! - Position persistence for crash recovery

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use crate::adapters::jupiter::{JupiterClient, QuoteRequest, SwapRequest};
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::adapters::pump_fun::{BondingCurveState, PumpFunTokenState};
use crate::adapters::market_data::JupiterPriceClient;
use crate::domain::{BalanceGuard, ExpectedDelta};
use crate::strategy::ou_process::{OUProcess, OUSignal, OUParams};

/// USDC mint address on Solana
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

/// Wrapped SOL mint address
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Default position persistence file
pub const POSITION_FILE: &str = "meme_position.json";

/// SOL/USDC price cache TTL in seconds
pub const SOL_PRICE_CACHE_TTL_SECS: u64 = 30;

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
}

impl Default for MemeOrchestratorConfig {
    fn default() -> Self {
        Self {
            ou_lookback: 100,
            ou_dt_minutes: 1.0,
            z_entry_threshold: -3.5,
            z_exit_threshold: 0.0,
            stop_loss_pct: 10.0,
            take_profit_pct: 15.0,
            max_position_hours: 4.0,
            trade_size_usdc: 50.0,
            slippage_bps: 100, // 1%
            poll_interval_secs: 60,
            priority_fee_lamports: 10_000,
            data_dir: PathBuf::from("data"),
            paper_mode: true,
            min_ou_confidence: 0.3,
            min_half_life_minutes: 5.0,
            max_half_life_minutes: 120.0,
        }
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
    /// Pump.fun bonding curve state (if token is from pump.fun)
    pub pump_fun_state: Option<PumpFunTokenState>,
}

impl TokenTracker {
    pub fn new(info: TokenInfo, ou_lookback: usize, ou_dt_minutes: f64) -> Self {
        Self {
            info,
            ou_process: OUProcess::new(ou_lookback, ou_dt_minutes),
            last_price_time: None,
            price_history: Vec::with_capacity(ou_lookback),
            max_history: ou_lookback * 2,
            pump_fun_state: None,
        }
    }

    /// Create a new tracker with pump.fun bonding curve state
    pub fn with_pump_fun(
        info: TokenInfo,
        ou_lookback: usize,
        ou_dt_minutes: f64,
        bonding_curve: BondingCurveState,
    ) -> Self {
        Self {
            info,
            ou_process: OUProcess::new(ou_lookback, ou_dt_minutes),
            last_price_time: None,
            price_history: Vec::with_capacity(ou_lookback),
            max_history: ou_lookback * 2,
            pump_fun_state: Some(PumpFunTokenState::new(bonding_curve)),
        }
    }

    /// Check if this is a pump.fun token still on the bonding curve
    pub fn is_pump_fun_pre_graduation(&self) -> bool {
        self.pump_fun_state
            .as_ref()
            .map(|s| s.is_on_bonding_curve())
            .unwrap_or(false)
    }

    /// Get the pump.fun price in SOL if available
    pub fn pump_fun_price_sol(&self) -> Option<f64> {
        self.pump_fun_state
            .as_ref()
            .filter(|s| s.is_on_bonding_curve())
            .map(|s| s.price_sol)
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

/// Cached SOL/USDC price with timestamp
struct SolPriceCache {
    price: f64,
    timestamp: Instant,
}

/// Meme coin trading orchestrator
pub struct MemeOrchestrator {
    /// Configuration
    config: MemeOrchestratorConfig,
    /// Jupiter client for swaps
    jupiter: JupiterClient,
    /// Jupiter price client for SOL/USDC
    price_client: JupiterPriceClient,
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
    /// Cached SOL/USDC price
    sol_price_cache: Arc<RwLock<Option<SolPriceCache>>>,
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
        let price_client = JupiterPriceClient::new()
            .map_err(|e| MemeOrchestratorError::MarketDataError(e.to_string()))?;

        Ok(Self {
            config,
            jupiter,
            price_client,
            solana,
            wallet: wallet.clone(),
            tokens: Arc::new(RwLock::new(HashMap::new())),
            active_position: Arc::new(RwLock::new(None)),
            trade_lock: Arc::new(Mutex::new(())),
            balance_guard: Arc::new(RwLock::new(balance_guard)),
            is_running: Arc::new(RwLock::new(false)),
            shutdown_requested: Arc::new(RwLock::new(false)),
            sol_price_cache: Arc::new(RwLock::new(None)),
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

    /// Add a pump.fun token with bonding curve state
    pub async fn add_pump_token(&self, info: TokenInfo, bonding_curve: BondingCurveState) {
        let tracker = TokenTracker::with_pump_fun(
            info.clone(),
            self.config.ou_lookback,
            self.config.ou_dt_minutes,
            bonding_curve.clone(),
        );

        let price_sol = tracker.pump_fun_price_sol().unwrap_or(0.0);
        let grad_progress = bonding_curve.graduation_progress();

        self.tokens.write().await.insert(info.mint.clone(), tracker);

        tracing::info!(
            "Added pump.fun token: {} ({}) - price: {} SOL, graduation: {:.1}%",
            info.symbol,
            info.mint,
            price_sol,
            grad_progress
        );
    }

    /// Update pump.fun state from a trade event
    ///
    /// Call this when receiving trade events from pump.fun WebSocket
    pub async fn update_pump_state(
        &self,
        mint: &str,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
        is_complete: bool,
        trade_ts: u64,
    ) -> Result<(), MemeOrchestratorError> {
        let mut tokens = self.tokens.write().await;
        let tracker = tokens
            .get_mut(mint)
            .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;

        if let Some(ref mut pump_state) = tracker.pump_fun_state {
            let old_price = pump_state.price_sol;
            pump_state.update_from_trade(
                virtual_sol_reserves,
                virtual_token_reserves,
                is_complete,
                trade_ts,
            );
            let new_price = pump_state.price_sol;

            tracing::debug!(
                "Updated pump.fun state for {}: {} SOL -> {} SOL (grad: {:.1}%)",
                tracker.info.symbol,
                old_price,
                new_price,
                pump_state.bonding_curve.graduation_progress()
            );

            // If graduated, log it
            if is_complete && !pump_state.bonding_curve.complete {
                tracing::info!(
                    "Token {} graduated from pump.fun! Switching to Jupiter pricing.",
                    tracker.info.symbol
                );
            }
        }

        Ok(())
    }

    /// Check if a token is a pump.fun pre-graduation token
    pub async fn is_pump_fun_pre_graduation(&self, mint: &str) -> bool {
        let tokens = self.tokens.read().await;
        tokens
            .get(mint)
            .map(|t| t.is_pump_fun_pre_graduation())
            .unwrap_or(false)
    }

    /// Get pump.fun graduation progress for a token (0-100%)
    pub async fn get_pump_graduation_progress(&self, mint: &str) -> Option<f64> {
        let tokens = self.tokens.read().await;
        tokens.get(mint).and_then(|t| {
            t.pump_fun_state
                .as_ref()
                .map(|s| s.bonding_curve.graduation_progress())
        })
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

    /// Fetch price for a token
    ///
    /// For pump.fun tokens still on the bonding curve, uses the bonding curve price.
    /// For graduated tokens or non-pump.fun tokens, uses Jupiter quote.
    pub async fn fetch_token_price(&self, mint: &str) -> Result<f64, MemeOrchestratorError> {
        // Extract what we need from the tracker
        let (info, pump_state) = {
            let tokens = self.tokens.read().await;
            let tracker = tokens
                .get(mint)
                .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;
            (tracker.info.clone(), tracker.pump_fun_state.clone())
        };

        // Check if this is a pump.fun token still on the bonding curve
        if let Some(ref pump_state) = pump_state {
            if pump_state.is_on_bonding_curve() {
                // Use bonding curve price
                let price_sol = pump_state.price_sol;
                let sol_usdc = self.get_sol_usdc_price().await?;
                let price_usdc = price_sol * sol_usdc;

                tracing::debug!(
                    "Pump.fun bonding curve price for {}: {} SOL = ${:.10} (grad: {:.1}%)",
                    info.symbol,
                    price_sol,
                    price_usdc,
                    pump_state.bonding_curve.graduation_progress()
                );

                return Ok(price_usdc);
            }
        }

        // Fall back to Jupiter Quote API
        self.fetch_jupiter_price(mint, &info).await
    }

    /// Fetch price using Jupiter quote (for graduated or non-pump.fun tokens)
    async fn fetch_jupiter_price(
        &self,
        mint: &str,
        info: &TokenInfo,
    ) -> Result<f64, MemeOrchestratorError> {
        let amount = 10u64.pow(info.decimals as u32); // 1 token

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

    /// Get SOL/USDC price with caching
    ///
    /// Caches the price for SOL_PRICE_CACHE_TTL_SECS to avoid excessive API calls.
    pub async fn get_sol_usdc_price(&self) -> Result<f64, MemeOrchestratorError> {
        // Check cache first
        {
            let cache = self.sol_price_cache.read().await;
            if let Some(ref cached) = *cache {
                if cached.timestamp.elapsed() < Duration::from_secs(SOL_PRICE_CACHE_TTL_SECS) {
                    return Ok(cached.price);
                }
            }
        }

        // Fetch fresh price
        let price = self
            .price_client
            .get_price(WSOL_MINT)
            .await
            .map_err(|e| MemeOrchestratorError::MarketDataError(e.to_string()))?;

        // Update cache
        {
            let mut cache = self.sol_price_cache.write().await;
            *cache = Some(SolPriceCache {
                price,
                timestamp: Instant::now(),
            });
        }

        tracing::debug!("Updated SOL/USDC price: ${:.2}", price);
        Ok(price)
    }

    /// Check for entry signal on a specific token
    pub async fn check_entry_signal(&self, mint: &str) -> Result<bool, MemeOrchestratorError> {
        // Can only enter if no position open
        if self.active_position.read().await.is_some() {
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

        // Check OU signal
        if let Some(z_score) = tracker.ou_process.current_z_score() {
            // Entry on oversold (z < threshold, e.g., z < -3.5)
            if z_score < self.config.z_entry_threshold {
                tracing::info!(
                    "Entry signal for {}: z={:.2} < {:.2}",
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

            // Check stop loss
            if pnl_pct <= -self.config.stop_loss_pct {
                tracing::warn!(
                    "Stop loss triggered for {}: {:.2}%",
                    position.token_symbol,
                    pnl_pct
                );
                return Ok(true);
            }

            // Check take profit
            if pnl_pct >= self.config.take_profit_pct {
                tracing::info!(
                    "Take profit triggered for {}: {:.2}%",
                    position.token_symbol,
                    pnl_pct
                );
                return Ok(true);
            }

            // Check time stop
            if age_hours >= self.config.max_position_hours {
                tracing::info!(
                    "Time stop triggered for {}: {:.1}h",
                    position.token_symbol,
                    age_hours
                );
                return Ok(true);
            }

            // Check z-score mean reversion exit
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

        let (tracker_info, z_score, ou_params) = {
            let tokens = self.tokens.read().await;
            let tracker = tokens
                .get(mint)
                .ok_or_else(|| MemeOrchestratorError::TokenNotFound(mint.to_string()))?;
            (
                tracker.info.clone(),
                tracker.ou_process.current_z_score().unwrap_or(0.0),
                tracker.ou_process.params().cloned(),
            )
        };

        if self.config.paper_mode {
            // Paper trade - simulate entry
            let price = tracker_info.price_usdc.unwrap_or(0.0);
            let size = ((self.config.trade_size_usdc / price)
                * 10f64.powi(tracker_info.decimals as i32)) as u64;

            let position = ActivePosition::new(
                mint.to_string(),
                tracker_info.symbol.clone(),
                price,
                size,
                self.config.trade_size_usdc,
                z_score,
                ou_params,
            );

            tracing::info!(
                "PAPER ENTRY: {} {} at ${:.8} (z={:.2})",
                tracker_info.symbol,
                size,
                price,
                z_score
            );

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
            .get_rpc_client()
            .get_balance(&self.wallet.pubkey())
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

        // Execute transaction (simplified - full implementation would sign and submit)
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

        let position = ActivePosition::new(
            mint.to_string(),
            tracker_info.symbol.clone(),
            price,
            output_amount,
            self.config.trade_size_usdc,
            z_score,
            ou_params,
        );

        // Validate post-trade balance (fees only, we're spending USDC not SOL)
        let post_balance = self
            .solana
            .get_rpc_client()
            .get_balance(&self.wallet.pubkey())
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
            .get_rpc_client()
            .get_balance(&self.wallet.pubkey())
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

        tracing::info!(
            "Executing exit swap: {} -> {:.2} USDC (PnL: {:.2}%)",
            position.token_symbol,
            output_usdc,
            pnl_pct
        );

        // Validate post-trade balance
        let post_balance = self
            .solana
            .get_rpc_client()
            .get_balance(&self.wallet.pubkey())
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
                        let params = tracker.ou_process.params();

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
        assert!(tracker.pump_fun_state.is_none());
    }

    #[test]
    fn test_token_tracker_with_pump_fun() {
        use crate::adapters::pump_fun::BondingCurveState;

        let info = create_test_token_info();
        let bonding_curve = BondingCurveState::new("TestMint123".to_string());
        let tracker = TokenTracker::with_pump_fun(info, 100, 1.0, bonding_curve);

        assert!(tracker.pump_fun_state.is_some());
        assert!(tracker.is_pump_fun_pre_graduation());
        assert!(tracker.pump_fun_price_sol().is_some());
    }

    #[test]
    fn test_token_tracker_pump_fun_graduation() {
        use crate::adapters::pump_fun::BondingCurveState;

        let info = create_test_token_info();
        let mut bonding_curve = BondingCurveState::new("TestMint123".to_string());
        bonding_curve.complete = true; // Graduated

        let tracker = TokenTracker::with_pump_fun(info, 100, 1.0, bonding_curve);

        assert!(tracker.pump_fun_state.is_some());
        assert!(!tracker.is_pump_fun_pre_graduation()); // Not pre-graduation
        assert!(tracker.pump_fun_price_sol().is_none()); // No bonding curve price
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
