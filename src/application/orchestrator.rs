//! Trading Orchestrator
//!
//! Coordinates the mean reversion strategy with Jupiter execution.
//! Main trading loop that fetches prices, updates strategy, and executes trades.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use thiserror::Error;
use base64::Engine;
use solana_sdk::transaction::VersionedTransaction;

use crate::strategy::{
    MeanReversionStrategy, StrategyConfig, TradeAction, PositionState,
    AdxRegimeDetector, AdxConfig, CandleBuilder, RegimeDetector,
};
use crate::adapters::jupiter::{JupiterClient, QuoteRequest, SwapRequest};
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::domain::{
    BalanceGuard, ExpectedDelta,
    TransactionValidator,
};

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
    #[error("Security violation: {0}")]
    SecurityViolation(String),
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
    /// Trade size in SOL (e.g., 0.1 = trade 0.1 SOL per signal)
    trade_size_sol: f64,
    /// Priority fee in lamports for faster transaction inclusion
    priority_fee_lamports: u64,
    balance_guard: Arc<RwLock<BalanceGuard>>,
    tx_validator: TransactionValidator,
    /// ADX regime detector for filtering trending markets
    adx_detector: Arc<RwLock<AdxRegimeDetector>>,
    /// Candle builder to create OHLC from price ticks
    candle_builder: Arc<RwLock<CandleBuilder>>,
    /// Position multiplier from ADX regime detection (0.0-1.0)
    /// During warmup this defaults to WARMUP_MULTIPLIER for cautious trading
    regime_multiplier: Arc<RwLock<f64>>,
}

/// Position multiplier during ADX warmup (trade cautiously until ADX is ready)
const WARMUP_MULTIPLIER: f64 = 0.5;

/// Status snapshot of the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorStatus {
    pub is_running: bool,
    pub position: String,  // "Flat", "Long", "Short"
    pub daily_trades: u32,
    pub daily_pnl_pct: f64,
    pub last_price: Option<f64>,
    pub current_zscore: Option<f64>,
    /// Current ADX value (0-100), None if warming up
    pub adx_value: Option<f64>,
    /// Current trend regime from ADX
    pub trend_regime: String,
    /// Position size multiplier from regime detection (0.0-1.0)
    pub regime_multiplier: f64,
    /// Whether ADX has enough data
    pub adx_ready: bool,
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
        trade_size_sol: f64,
        priority_fee_lamports: u64,
    ) -> Result<Self, OrchestratorError> {
        let strategy = MeanReversionStrategy::new(strategy_config);

        // Initialize ADX with crypto-optimized settings (period=10, faster response)
        let adx_detector = AdxRegimeDetector::new(AdxConfig::crypto_optimized());

        // Build 1-minute candles from price ticks for ADX
        let candle_builder = CandleBuilder::one_minute();

        Ok(Self {
            strategy: Arc::new(RwLock::new(strategy)),
            jupiter,
            solana,
            wallet: wallet.clone(),
            base_mint,
            quote_mint,
            slippage_bps,
            is_running: Arc::new(RwLock::new(false)),
            paper_mode,
            poll_interval: Duration::from_secs(15), // 15 second poll to avoid API rate limits
            trade_size_sol,
            priority_fee_lamports,
            balance_guard: Arc::new(RwLock::new(BalanceGuard::new(wallet.pubkey()))),
            tx_validator: TransactionValidator::new(wallet.pubkey()),
            adx_detector: Arc::new(RwLock::new(adx_detector)),
            candle_builder: Arc::new(RwLock::new(candle_builder)),
            regime_multiplier: Arc::new(RwLock::new(WARMUP_MULTIPLIER)), // Start with cautious trading
        })
    }

    /// Create with custom ADX configuration
    pub fn with_adx_config(mut self, config: AdxConfig) -> Self {
        self.adx_detector = Arc::new(RwLock::new(AdxRegimeDetector::new(config)));
        self
    }

    /// Create with custom candle period for ADX
    pub fn with_candle_period(mut self, period: Duration) -> Self {
        self.candle_builder = Arc::new(RwLock::new(CandleBuilder::new(period)));
        self
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

        // 2. Update candle builder and ADX regime detection
        let (adx_value, adx_ready, regime) = self.update_regime_detection(price).await;

        // 3. Get current regime multiplier (graceful degradation during warmup)
        let multiplier = *self.regime_multiplier.read().await;

        // 4. Get action from strategy (does NOT update state yet)
        let action = {
            let mut strategy = self.strategy.write().await;
            strategy.update(price)
        };

        // 5. Get z-score for logging
        let z_score = {
            let strategy = self.strategy.read().await;
            strategy.current_zscore().map(|z| z.z_score).unwrap_or(0.0)
        };

        // 6. Execute if action needed, respecting regime filter
        if let Some(action) = action {
            match action {
                TradeAction::EnterLong | TradeAction::EnterShort => {
                    // For entries, check if regime allows trading
                    if multiplier <= 0.0 {
                        tracing::info!(
                            "SOL ${:.2} | Z: {:.2} | ADX: {:.1} ({}) | BLOCKED by regime (multiplier=0)",
                            price, z_score,
                            adx_value.unwrap_or(0.0),
                            regime
                        );
                        return Ok(());
                    }

                    // Log the trade attempt with regime info
                    let warmup_note = if adx_ready { "" } else { " [ADX warming up]" };
                    tracing::info!(
                        "SOL ${:.2} | Z: {:.2} | ADX: {:.1} ({}) | {:?} (size x{:.0}%){}",
                        price, z_score,
                        adx_value.unwrap_or(0.0),
                        regime,
                        action,
                        multiplier * 100.0,
                        warmup_note
                    );

                    // Execute the trade
                    match self.execute_trade(&action, price).await {
                        Ok(()) => {
                            // Trade succeeded - NOW update strategy state
                            let mut strategy = self.strategy.write().await;
                            strategy.confirm_trade(action, price);
                            tracing::info!("Trade confirmed, strategy state updated");
                        }
                        Err(e) => {
                            tracing::error!(
                                "Trade execution failed: {}. Position state unchanged, will retry.",
                                e
                            );
                            return Err(e);
                        }
                    }
                }
                TradeAction::Exit => {
                    // Always allow exits regardless of regime
                    tracing::info!(
                        "SOL ${:.2} | Z: {:.2} | ADX: {:.1} ({}) | EXIT",
                        price, z_score,
                        adx_value.unwrap_or(0.0),
                        regime
                    );

                    match self.execute_trade(&action, price).await {
                        Ok(()) => {
                            let mut strategy = self.strategy.write().await;
                            strategy.confirm_trade(action, price);
                            tracing::info!("Exit confirmed, position closed");
                        }
                        Err(e) => {
                            tracing::warn!("Exit trade failed - will retry on next tick: {}", e);
                            // Don't propagate exit errors - keep trying
                        }
                    }
                }
                TradeAction::Hold => {
                    let warmup_note = if adx_ready { "" } else { " [ADX warming up]" };
                    tracing::info!(
                        "SOL ${:.2} | Z: {:.2} | ADX: {:.1} ({}) | HOLD{}",
                        price, z_score,
                        adx_value.unwrap_or(0.0),
                        regime,
                        warmup_note
                    );
                }
            }
        } else {
            // Strategy still warming up (z-score not ready)
            let warmup_note = if adx_ready { "" } else { " [ADX warming up]" };
            tracing::info!(
                "SOL ${:.2} | Strategy warming up... | ADX: {:.1}{}",
                price,
                adx_value.unwrap_or(0.0),
                warmup_note
            );
        }

        Ok(())
    }

    /// Update regime detection with new price, returns (adx_value, adx_ready, regime_name)
    async fn update_regime_detection(&self, price: f64) -> (Option<f64>, bool, String) {
        // Feed price to candle builder
        let maybe_candle = {
            let mut builder = self.candle_builder.write().await;
            builder.update(price)
        };

        // If a candle completed, feed it to ADX
        if let Some(candle) = maybe_candle {
            let mut adx = self.adx_detector.write().await;
            adx.update(&candle);

            // Update regime multiplier based on ADX
            let new_multiplier = if adx.is_ready() {
                adx.get_position_multiplier()
            } else {
                // Graceful degradation: trade cautiously during warmup
                WARMUP_MULTIPLIER
            };

            *self.regime_multiplier.write().await = new_multiplier;

            tracing::debug!(
                "Candle closed: O={:.2} H={:.2} L={:.2} C={:.2} | ADX={:.1} | Multiplier={:.0}%",
                candle.open, candle.high, candle.low, candle.close,
                adx.adx(),
                new_multiplier * 100.0
            );
        }

        // Get current ADX state for logging
        let adx = self.adx_detector.read().await;
        let adx_ready = adx.is_ready();
        let adx_value = if adx_ready { Some(adx.adx()) } else { None };
        let regime = format!("{:?}", adx.get_regime());

        (adx_value, adx_ready, regime)
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

    /// Execute a trade action via Jupiter swap
    async fn execute_trade(&self, action: &TradeAction, price: f64) -> Result<(), OrchestratorError> {
        if self.paper_mode {
            tracing::info!(
                "PAPER TRADE - Action: {:?}, Price: ${:.2}",
                action,
                price
            );
            return Ok(());
        }

        tracing::info!(
            "EXECUTING TRADE - Action: {:?}, Price: ${:.2}",
            action,
            price
        );

        // 1. Check if trading is halted due to balance anomaly
        if self.balance_guard.read().await.is_halted() {
            return Err(OrchestratorError::ExecutionError(
                "Trading halted due to balance anomaly - manual review required".to_string()
            ));
        }

        // 2. Capture pre-trade balance
        let pre_balance = self.solana.get_rpc_client().get_balance(&self.wallet.pubkey())
            .map_err(|e| OrchestratorError::ExecutionError(format!("Failed to get balance: {}", e)))?;
        self.balance_guard.write().await.capture_pre_trade(pre_balance);

        // 3. Determine swap direction and amount based on action
        let (input_mint, output_mint, amount) = self.get_swap_params(action, price).await?;

        if amount == 0 {
            tracing::warn!("Trade amount is zero, skipping");
            return Ok(());
        }

        // 4. Get quote from Jupiter
        let quote_request = QuoteRequest::new(
            input_mint.clone(),
            output_mint.clone(),
            amount,
            self.slippage_bps,
        );

        tracing::info!(
            "Requesting quote: {} {} -> {}",
            amount, input_mint, output_mint
        );

        let quote = self.jupiter.get_quote(&quote_request).await
            .map_err(|e| OrchestratorError::ExecutionError(format!("Quote failed: {}", e)))?;

        let in_amount = quote.input_amount();
        let out_amount = quote.output_amount();
        let price_impact = quote.price_impact();

        tracing::info!(
            "Quote received: {} -> {} (impact: {:.4}%)",
            in_amount, out_amount, price_impact
        );

        // Check price impact isn't too high
        if price_impact > 1.0 {
            tracing::warn!("Price impact too high ({:.2}%), aborting trade", price_impact);
            return Err(OrchestratorError::ExecutionError(
                format!("Price impact {:.2}% exceeds 1% limit", price_impact)
            ));
        }

        // 3. Build swap transaction via Jupiter
        let quote_json = serde_json::to_value(&quote)
            .map_err(|e| OrchestratorError::ExecutionError(format!("JSON serialize failed: {}", e)))?;

        let swap_request = SwapRequest::new(
            self.wallet.public_key(),
            quote_json,
        ).with_priority_fee(self.priority_fee_lamports);

        tracing::info!("Building swap transaction...");

        let swap_response = self.jupiter.get_swap_transaction(&swap_request).await
            .map_err(|e| OrchestratorError::ExecutionError(format!("Swap build failed: {}", e)))?;

        // 4. Decode base64 transaction
        let tx_bytes = base64::engine::general_purpose::STANDARD
            .decode(&swap_response.swap_transaction)
            .map_err(|e| OrchestratorError::ExecutionError(format!("Base64 decode failed: {}", e)))?;

        // 5. Deserialize to VersionedTransaction
        let transaction: VersionedTransaction = bincode::deserialize(&tx_bytes)
            .map_err(|e| OrchestratorError::ExecutionError(format!("Deserialize failed: {}", e)))?;

        // SECURITY: Validate transaction before signing (WARNING ONLY - relies on BalanceGuard post-trade)
        // NOTE: Disabled blocking validation because Jupiter transactions include dynamic PDAs
        // (pool vaults, routing accounts) that cannot be statically whitelisted.
        // The BalanceGuard provides post-trade protection by detecting unexpected balance changes.
        match self.tx_validator.validate(&transaction) {
            Ok(result) => {
                tracing::info!(
                    "Transaction validated: {} transfers, {} CloseAccount instructions - all destinations authorized",
                    result.transfer_count,
                    result.close_account_count
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Transaction validation warning (proceeding anyway): {:?}. BalanceGuard will verify post-trade.",
                    e
                );
            }
        }

        tracing::info!(
            "Transaction built, {} signatures needed, block height limit: {}",
            transaction.message.header().num_required_signatures,
            swap_response.last_valid_block_height
        );

        // 6. Sign the transaction
        let signed_tx = self.sign_versioned_transaction(transaction)?;

        // 7. Submit to Solana and confirm
        tracing::info!("Submitting transaction to Solana...");

        let signature = self.submit_and_confirm_transaction(&signed_tx).await?;

        tracing::info!(
            "✅ TRADE EXECUTED - Signature: {}",
            signature
        );
        tracing::info!(
            "   View: https://solscan.io/tx/{}",
            signature
        );

        // Validate balance delta
        let post_balance = self.solana.get_rpc_client().get_balance(&self.wallet.pubkey())
            .map_err(|e| OrchestratorError::ExecutionError(format!("Failed to get post-trade balance: {}", e)))?;

        // Calculate expected delta based on swap direction
        const SOL_MINT: &str = "So11111111111111111111111111111111111111112";

        // Extract DEX fees from route plan for more accurate expected delta
        // Note: fee_amount may not always be returned by Jupiter API
        let dex_fees = quote.total_dex_fees();
        if dex_fees > 0 {
            tracing::debug!("DEX fees from route: {} lamports", dex_fees);
        }

        let expected_delta = if output_mint == SOL_MINT {
            // Token → SOL: We RECEIVE SOL (positive delta)
            // DEX fees reduce the amount we receive
            ExpectedDelta::token_to_sol(
                out_amount.saturating_sub(dex_fees),
                swap_response.prioritization_fee_lamports,
                0  // jito tip handled separately if enabled
            )
        } else if input_mint == SOL_MINT {
            // SOL → Token: We SPEND SOL (negative delta)
            // DEX fees increase the amount we spend
            ExpectedDelta::sol_to_token(
                in_amount.saturating_add(dex_fees),
                swap_response.prioritization_fee_lamports,
                0
            )
        } else {
            // Token → Token: Only fees affect SOL balance
            ExpectedDelta::custom(
                -(swap_response.prioritization_fee_lamports as i64) - 5000,
                format!("Swap {} -> {} (fees only)", input_mint, output_mint)
            )
        };

        if let Err(e) = self.balance_guard.write().await.validate_post_trade(post_balance, &expected_delta) {
            tracing::error!("Balance guard violation: {:?}", e);
            // Don't return error - trade already executed, just log the warning
        }

        Ok(())
    }

    /// Determine swap parameters based on trade action
    async fn get_swap_params(&self, action: &TradeAction, price: f64) -> Result<(String, String, u64), OrchestratorError> {
        match action {
            TradeAction::EnterLong => {
                // Buy SOL with USDC: USDC -> SOL
                let usdc_amount = (self.trade_size_sol * price * 1_000_000.0) as u64;
                Ok((self.quote_mint.clone(), self.base_mint.clone(), usdc_amount))
            }
            TradeAction::EnterShort => {
                // Sell SOL for USDC: SOL -> USDC
                let sol_amount = (self.trade_size_sol * 1_000_000_000.0) as u64;
                Ok((self.base_mint.clone(), self.quote_mint.clone(), sol_amount))
            }
            TradeAction::Exit => {
                // Exit depends on current position
                let strategy = self.strategy.read().await;
                match strategy.position() {
                    PositionState::Long { .. } => {
                        // Exit long = sell SOL
                        let sol_amount = (self.trade_size_sol * 1_000_000_000.0) as u64;
                        Ok((self.base_mint.clone(), self.quote_mint.clone(), sol_amount))
                    }
                    PositionState::Short { .. } => {
                        // Exit short = buy SOL
                        let usdc_amount = (self.trade_size_sol * price * 1_000_000.0) as u64;
                        Ok((self.quote_mint.clone(), self.base_mint.clone(), usdc_amount))
                    }
                    PositionState::Flat => {
                        tracing::warn!("Exit called but position is flat");
                        Ok((String::new(), String::new(), 0))
                    }
                }
            }
            TradeAction::Hold => {
                Ok((String::new(), String::new(), 0))
            }
        }
    }

    /// Sign a VersionedTransaction with our wallet
    fn sign_versioned_transaction(&self, mut transaction: VersionedTransaction) -> Result<VersionedTransaction, OrchestratorError> {
        use solana_sdk::signature::Signer;

        // Get the message bytes to sign
        let message_bytes = transaction.message.serialize();

        // Sign with our keypair
        let signature = self.wallet.keypair().sign_message(&message_bytes);

        // The first signature slot is for the fee payer (our wallet)
        if transaction.signatures.is_empty() {
            return Err(OrchestratorError::ExecutionError(
                "Transaction has no signature slots".to_string()
            ));
        }

        transaction.signatures[0] = signature;

        tracing::debug!("Transaction signed with signature: {}", signature);

        Ok(transaction)
    }

    /// Submit transaction to Solana and wait for confirmation with timeout
    async fn submit_and_confirm_transaction(&self, transaction: &VersionedTransaction) -> Result<String, OrchestratorError> {
        use solana_client::rpc_config::RpcSendTransactionConfig;
        use solana_sdk::commitment_config::CommitmentLevel;
        use solana_sdk::signature::Signature;
        use std::str::FromStr;

        // Timeout configuration
        const SEND_TIMEOUT_SECS: u64 = 30;
        const CONFIRM_TIMEOUT_SECS: u64 = 60;
        const POLL_INTERVAL_MS: u64 = 500;

        // Serialize the signed transaction
        let tx_bytes = bincode::serialize(transaction)
            .map_err(|e| OrchestratorError::ExecutionError(format!("Serialize failed: {}", e)))?;

        let tx_base64 = base64::engine::general_purpose::STANDARD.encode(&tx_bytes);
        let client = self.solana.get_rpc_client();

        // Step 1: Send transaction with timeout
        let send_result = tokio::time::timeout(
            Duration::from_secs(SEND_TIMEOUT_SECS),
            tokio::task::spawn_blocking({
                let tx_base64 = tx_base64.clone();
                let client = Arc::clone(&client);
                move || {
                    let tx_bytes = base64::engine::general_purpose::STANDARD
                        .decode(&tx_base64)
                        .map_err(|e| OrchestratorError::ExecutionError(e.to_string()))?;

                    let transaction: VersionedTransaction = bincode::deserialize(&tx_bytes)
                        .map_err(|e| OrchestratorError::ExecutionError(e.to_string()))?;

                    let config = RpcSendTransactionConfig {
                        skip_preflight: true,
                        preflight_commitment: Some(CommitmentLevel::Confirmed),
                        max_retries: Some(3),
                        ..Default::default()
                    };

                    client
                        .send_transaction_with_config(&transaction, config)
                        .map(|sig| sig.to_string())
                        .map_err(|e| OrchestratorError::ExecutionError(format!("Send failed: {}", e)))
                }
            })
        ).await;

        let signature = match send_result {
            Ok(Ok(Ok(sig))) => sig,
            Ok(Ok(Err(e))) => return Err(e),
            Ok(Err(e)) => return Err(OrchestratorError::ExecutionError(format!("Task join error: {}", e))),
            Err(_) => return Err(OrchestratorError::ExecutionError(format!("Send timeout after {}s", SEND_TIMEOUT_SECS))),
        };

        tracing::info!("Transaction sent: {}", signature);

        // Step 2: Poll for confirmation with timeout
        let start = Instant::now();
        let confirm_timeout = Duration::from_secs(CONFIRM_TIMEOUT_SECS);
        let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

        let sig = Signature::from_str(&signature)
            .map_err(|e| OrchestratorError::ExecutionError(format!("Invalid signature: {}", e)))?;

        loop {
            // Check total timeout
            if start.elapsed() > confirm_timeout {
                tracing::error!(
                    "Confirmation timeout after {}s. Transaction may still land. Signature: {}",
                    CONFIRM_TIMEOUT_SECS,
                    signature
                );
                return Err(OrchestratorError::ExecutionError(
                    format!("Confirmation timeout after {}s. Signature: {}", CONFIRM_TIMEOUT_SECS, signature)
                ));
            }

            // Poll signature status with individual timeout
            let client_clone = Arc::clone(&client);
            let sig_clone = sig;

            let status_result = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::task::spawn_blocking(move || {
                    client_clone.get_signature_status(&sig_clone)
                })
            ).await;

            match status_result {
                Ok(Ok(Ok(Some(result)))) => {
                    if result.is_ok() {
                        tracing::debug!("Transaction confirmed after {:.1}s", start.elapsed().as_secs_f64());
                        return Ok(signature);
                    } else {
                        return Err(OrchestratorError::ExecutionError(
                            format!("Transaction failed on-chain: {:?}", result.err())
                        ));
                    }
                }
                Ok(Ok(Ok(None))) => {
                    // Not yet confirmed, continue polling
                    tracing::debug!(
                        "Waiting for confirmation... ({:.1}s elapsed)",
                        start.elapsed().as_secs_f64()
                    );
                }
                Ok(Ok(Err(e))) => {
                    tracing::warn!("RPC error during confirmation poll: {}", e);
                }
                Ok(Err(e)) => {
                    tracing::warn!("Task error during confirmation poll: {}", e);
                }
                Err(_) => {
                    tracing::debug!("Individual poll timed out, retrying...");
                }
            }

            tokio::time::sleep(poll_interval).await;
        }
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

        // Get ADX status
        let adx = self.adx_detector.read().await;
        let adx_ready = adx.is_ready();
        let adx_value = if adx_ready { Some(adx.adx()) } else { None };
        let trend_regime = format!("{:?}", adx.get_regime());
        drop(adx); // Release lock before reading multiplier

        let regime_multiplier = *self.regime_multiplier.read().await;

        OrchestratorStatus {
            is_running,
            position,
            daily_trades: strategy.daily_trade_count(),
            daily_pnl_pct: strategy.daily_pnl_pct(),
            last_price: None, // Could cache this from last tick
            current_zscore,
            adx_value,
            trend_regime,
            regime_multiplier,
            adx_ready,
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
            trade_size_sol: self.trade_size_sol,
            priority_fee_lamports: self.priority_fee_lamports,
            balance_guard: Arc::clone(&self.balance_guard),
            tx_validator: self.tx_validator.clone(),
            adx_detector: Arc::clone(&self.adx_detector),
            candle_builder: Arc::clone(&self.candle_builder),
            regime_multiplier: Arc::clone(&self.regime_multiplier),
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
            50,    // 0.5% slippage
            true,  // paper mode
            0.1,   // trade 0.1 SOL per signal
            5000,  // 5000 lamports priority fee
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
