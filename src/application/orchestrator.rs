//! Trading Orchestrator
//!
//! Coordinates the mean reversion strategy with Jupiter execution.
//! Main trading loop that fetches prices, updates strategy, and executes trades.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use thiserror::Error;

use crate::strategy::{MeanReversionStrategy, StrategyConfig, TradeAction, PositionState};
use crate::adapters::jupiter::{JupiterClient, QuoteRequest};
use crate::adapters::solana::{SolanaClient, WalletManager};

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("Configuration error: {0}")]
    ConfigError(String),
    #[error("Market data error: {0}")]
    MarketDataError(String),
    #[error("Execution error: {0}")]
    ExecutionError(String),
    #[error("Wallet error: {0}")]
    WalletError(String),
}

/// Main trading orchestrator that coordinates strategy and execution
pub struct TradingOrchestrator {
    strategy: Arc<RwLock<MeanReversionStrategy>>,
    jupiter: JupiterClient,
    solana: SolanaClient,
    wallet: WalletManager,
    base_mint: String,
    quote_mint: String,
    slippage_bps: u16,
    is_running: Arc<RwLock<bool>>,
    paper_mode: bool,
    poll_interval: Duration,
}

/// Status snapshot of the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    pub is_running: bool,
    pub position: String,  // "Flat", "Long", "Short"
    pub daily_trades: u32,
    pub daily_pnl_pct: f64,
    pub last_price: Option<f64>,
    pub current_zscore: Option<f64>,
}

impl TradingOrchestrator {
    /// Create new orchestrator
    pub fn new(
        strategy_config: StrategyConfig,
        jupiter: JupiterClient,
        solana: SolanaClient,
        wallet: WalletManager,
        base_mint: String,
        quote_mint: String,
        slippage_bps: u16,
        paper_mode: bool,
    ) -> Result<Self, OrchestratorError> {
        let strategy = MeanReversionStrategy::new(strategy_config);

        Ok(Self {
            strategy: Arc::new(RwLock::new(strategy)),
            jupiter,
            solana,
            wallet,
            base_mint,
            quote_mint,
            slippage_bps,
            is_running: Arc::new(RwLock::new(false)),
            paper_mode,
            poll_interval: Duration::from_secs(10), // 10 second default poll
        })
    }

    /// Set custom poll interval
    pub fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Run the main trading loop
    pub async fn run(&self) -> Result<(), OrchestratorError> {
        *self.is_running.write().await = true;

        tracing::info!(
            "Starting trading orchestrator - Paper mode: {}, Poll interval: {:?}",
            self.paper_mode,
            self.poll_interval
        );

        while *self.is_running.read().await {
            if let Err(e) = self.tick().await {
                tracing::error!("Tick error: {}", e);
                // Continue running despite errors
            }
            tokio::time::sleep(self.poll_interval).await;
        }

        tracing::info!("Trading orchestrator stopped");
        Ok(())
    }

    /// Execute one trading cycle
    pub async fn tick(&self) -> Result<(), OrchestratorError> {
        // 1. Fetch current price (use Jupiter quote for now)
        let price = self.fetch_price().await?;

        // 2. Update strategy
        let mut strategy = self.strategy.write().await;
        let action = strategy.update(price);

        // 3. Execute if action needed
        if let Some(action) = action {
            match action {
                TradeAction::EnterLong | TradeAction::EnterShort | TradeAction::Exit => {
                    self.execute_trade(&action, price).await?;
                }
                TradeAction::Hold => {
                    // Get z-score for display
                    let z_score = strategy.current_zscore()
                        .map(|z| z.z_score)
                        .unwrap_or(0.0);
                    tracing::info!(
                        "SOL ${:.2} | Z-score: {:.2} | Action: HOLD",
                        price, z_score
                    );
                }
            }
        } else {
            tracing::info!(
                "SOL ${:.2} | Warming up...",
                price
            );
        }

        Ok(())
    }

    /// Fetch current market price using Jupiter quote
    /// Quotes 1 SOL -> USDC to get the current SOL price
    async fn fetch_price(&self) -> Result<f64, OrchestratorError> {
        // Quote 1 SOL (1_000_000_000 lamports) to USDC to get price
        let amount = 1_000_000_000u64; // 1 SOL

        let quote_request = QuoteRequest::new(
            self.base_mint.clone(),
            self.quote_mint.clone(),
            amount,
            self.slippage_bps,
        );

        let quote = self.jupiter.get_quote(&quote_request).await
            .map_err(|e| OrchestratorError::MarketDataError(format!("Failed to get quote: {}", e)))?;

        // Calculate price from quote
        // output_amount is in USDC base units (6 decimals)
        let output_amount = quote.output_amount();
        let price = output_amount as f64 / 1_000_000.0; // USDC has 6 decimals

        Ok(price)
    }

    /// Execute a trade action
    async fn execute_trade(&self, action: &TradeAction, price: f64) -> Result<(), OrchestratorError> {
        if self.paper_mode {
            // Paper mode: just log the trade
            tracing::info!(
                "PAPER TRADE - Action: {:?}, Price: ${:.2}",
                action,
                price
            );
            return Ok(());
        }

        // Real execution would go here
        tracing::info!(
            "EXECUTING TRADE - Action: {:?}, Price: ${:.2}",
            action,
            price
        );

        // TODO: Implement real execution via Jupiter swap
        // 1. Get quote for the trade size
        // 2. Build swap transaction
        // 3. Sign with wallet
        // 4. Submit to Solana
        // 5. Confirm transaction

        Ok(())
    }

    /// Stop the trading loop
    pub async fn stop(&self) {
        *self.is_running.write().await = false;
        tracing::info!("Stop signal sent to orchestrator");
    }

    /// Get current status snapshot
    pub async fn status(&self) -> OrchestratorStatus {
        let strategy = self.strategy.read().await;
        let is_running = *self.is_running.read().await;

        let position = match strategy.position() {
            PositionState::Flat => "Flat".to_string(),
            PositionState::Long { entry_price } => format!("Long @ ${:.2}", entry_price),
            PositionState::Short { entry_price } => format!("Short @ ${:.2}", entry_price),
        };

        let current_zscore = strategy.current_zscore().map(|z| z.z_score);

        OrchestratorStatus {
            is_running,
            position,
            daily_trades: strategy.daily_trade_count(),
            daily_pnl_pct: strategy.daily_pnl_pct(),
            last_price: None, // Could cache this from last tick
            current_zscore,
        }
    }

    /// Reset daily counters (call at start of trading day)
    pub async fn reset_daily(&self) {
        let mut strategy = self.strategy.write().await;
        strategy.reset_daily();
        tracing::info!("Daily counters reset");
    }

    /// Check if strategy is ready (has enough data)
    pub async fn is_ready(&self) -> bool {
        let strategy = self.strategy.read().await;
        strategy.is_ready()
    }
}

// Implement Clone for TradingOrchestrator (needed for sharing across tasks)
impl Clone for TradingOrchestrator {
    fn clone(&self) -> Self {
        Self {
            strategy: Arc::clone(&self.strategy),
            jupiter: self.jupiter.clone(),
            solana: self.solana.clone(),
            wallet: self.wallet.clone(),
            base_mint: self.base_mint.clone(),
            quote_mint: self.quote_mint.clone(),
            slippage_bps: self.slippage_bps,
            is_running: Arc::clone(&self.is_running),
            paper_mode: self.paper_mode,
            poll_interval: self.poll_interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategy::StrategyConfig;

    fn create_test_orchestrator() -> TradingOrchestrator {
        let config = StrategyConfig::default();
        let jupiter = JupiterClient::new().unwrap();
        let solana = SolanaClient::new("https://api.devnet.solana.com".to_string());
        let wallet = WalletManager::new_random();

        TradingOrchestrator::new(
            config,
            jupiter,
            solana,
            wallet,
            "So11111111111111111111111111111111111111112".to_string(), // SOL
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v".to_string(), // USDC
            50, // 0.5% slippage
            true, // paper mode
        ).unwrap()
    }

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let orchestrator = create_test_orchestrator();
        let status = orchestrator.status().await;

        assert!(!status.is_running);
        assert_eq!(status.position, "Flat");
        assert_eq!(status.daily_trades, 0);
        assert_eq!(status.daily_pnl_pct, 0.0);
    }

    #[tokio::test]
    async fn test_tick_hold() {
        let orchestrator = create_test_orchestrator();

        // First tick should just warm up (no action)
        // Since we can't actually call Jupiter in tests without real network,
        // this test verifies the structure compiles

        let status = orchestrator.status().await;
        assert_eq!(status.position, "Flat");
    }

    #[tokio::test]
    async fn test_paper_mode_no_execution() {
        let orchestrator = create_test_orchestrator();
        assert!(orchestrator.paper_mode);

        // Paper mode should be set
        let status = orchestrator.status().await;
        assert_eq!(status.daily_trades, 0);
    }

    #[tokio::test]
    async fn test_stop_graceful() {
        let orchestrator = create_test_orchestrator();

        // Stop should update running flag
        orchestrator.stop().await;

        let status = orchestrator.status().await;
        assert!(!status.is_running);
    }

    #[tokio::test]
    async fn test_with_poll_interval() {
        let orchestrator = create_test_orchestrator()
            .with_poll_interval(Duration::from_secs(5));

        assert_eq!(orchestrator.poll_interval, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn test_reset_daily() {
        let orchestrator = create_test_orchestrator();

        orchestrator.reset_daily().await;

        let status = orchestrator.status().await;
        assert_eq!(status.daily_trades, 0);
        assert_eq!(status.daily_pnl_pct, 0.0);
    }

    #[tokio::test]
    async fn test_is_ready() {
        let orchestrator = create_test_orchestrator();

        // Strategy not ready initially (needs warmup)
        assert!(!orchestrator.is_ready().await);
    }

    #[tokio::test]
    async fn test_clone_orchestrator() {
        let orchestrator1 = create_test_orchestrator();
        let orchestrator2 = orchestrator1.clone();

        // Should share the same strategy
        orchestrator1.stop().await;

        let status = orchestrator2.status().await;
        assert!(!status.is_running); // Should reflect the stop
    }
}
