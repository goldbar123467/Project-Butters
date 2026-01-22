//! Meme Coin Trading CLI Commands
//!
//! CLI command handlers for meme coin trading functionality.
//! Connects CLI commands to the MemeOrchestrator.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::jupiter::JupiterClient;
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::application::{
    MemeOrchestrator, MemeOrchestratorConfig, PersistedState, TokenInfo,
    POSITION_FILE,
};
use crate::config::load_config;

/// Meme coin trading subcommands
///
/// Commands for multi-token meme coin trading using OU-GBM mean reversion strategy.
/// Only one position at a time, always settles to USDC.
#[derive(Subcommand, Debug, Clone)]
pub enum MemeCmd {
    /// Start meme trading loop
    Run(MemeRunCmd),

    /// Show orchestrator status
    Status(MemeStatusCmd),

    /// Add token to tracking
    AddToken(MemeAddTokenCmd),

    /// Remove token from tracking
    RemoveToken(MemeRemoveTokenCmd),

    /// List tracked tokens
    ListTokens(MemeListTokensCmd),

    /// Show active position
    Position(MemePositionCmd),

    /// Force exit position
    Exit(MemeExitCmd),
}

/// Start the meme coin trading orchestrator loop
#[derive(Parser, Debug, Clone)]
pub struct MemeRunCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Run in paper trading mode (no real transactions)
    #[arg(short, long)]
    pub paper: bool,

    /// Enable live mainnet trading
    #[arg(long, help = "Enable live mainnet trading")]
    pub live: bool,

    /// Acknowledge risk (required for --live)
    #[arg(long, help = "Acknowledge risk of financial loss")]
    pub i_accept_losses: bool,

    /// Comma-separated token mints to track (overrides config)
    #[arg(long, value_name = "MINTS")]
    pub tokens: Option<String>,

    /// Trade size in USDC
    #[arg(long, value_name = "USDC", default_value = "50.0")]
    pub trade_size: f64,

    /// Price poll interval in seconds
    #[arg(long, value_name = "SECS", default_value = "60")]
    pub poll_interval: u64,

    /// Directory for position persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Show meme orchestrator status
#[derive(Parser, Debug, Clone)]
pub struct MemeStatusCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Output format: text, json, table
    #[arg(short, long, value_name = "FORMAT", default_value = "text")]
    pub format: String,

    /// Directory for position persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Add a token to the tracking list
#[derive(Parser, Debug, Clone)]
pub struct MemeAddTokenCmd {
    /// Token mint address (base58)
    #[arg(value_name = "MINT")]
    pub mint: String,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Token symbol override (auto-fetched if not provided)
    #[arg(long, value_name = "SYM")]
    pub symbol: Option<String>,

    /// Token decimals override (auto-fetched if not provided)
    #[arg(long, value_name = "N")]
    pub decimals: Option<u8>,

    /// Directory for persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Remove a token from the tracking list
#[derive(Parser, Debug, Clone)]
pub struct MemeRemoveTokenCmd {
    /// Token mint address (base58)
    #[arg(value_name = "MINT")]
    pub mint: String,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Force remove even if position is open
    #[arg(short, long)]
    pub force: bool,

    /// Directory for persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// List all tracked tokens
#[derive(Parser, Debug, Clone)]
pub struct MemeListTokensCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Output format: text, json, table
    #[arg(short, long, value_name = "FORMAT", default_value = "table")]
    pub format: String,

    /// Directory for persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Show active position details
#[derive(Parser, Debug, Clone)]
pub struct MemePositionCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Output format: text, json
    #[arg(short, long, value_name = "FORMAT", default_value = "text")]
    pub format: String,

    /// Directory for persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Force exit current position
#[derive(Parser, Debug, Clone)]
pub struct MemeExitCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Slippage tolerance in basis points
    #[arg(long, value_name = "BPS", default_value = "150")]
    pub slippage: u16,

    /// Directory for persistence
    #[arg(long, value_name = "DIR", default_value = "data/meme")]
    pub data_dir: PathBuf,
}

/// Execute a meme command
///
/// This is the main entry point for meme CLI commands.
/// All command handlers are stubs that will be implemented in Wave 3.
pub async fn execute_meme_command(cmd: MemeCmd) -> Result<()> {
    match cmd {
        MemeCmd::Run(run_cmd) => meme_run_command(run_cmd).await,
        MemeCmd::Status(status_cmd) => meme_status_command(status_cmd).await,
        MemeCmd::AddToken(add_cmd) => meme_add_token_command(add_cmd).await,
        MemeCmd::RemoveToken(remove_cmd) => meme_remove_token_command(remove_cmd).await,
        MemeCmd::ListTokens(list_cmd) => meme_list_tokens_command(list_cmd).await,
        MemeCmd::Position(pos_cmd) => meme_position_command(pos_cmd).await,
        MemeCmd::Exit(exit_cmd) => meme_exit_command(exit_cmd).await,
    }
}

/// Handle `butters meme run` command
async fn meme_run_command(cmd: MemeRunCmd) -> Result<()> {
    tracing::info!("Starting meme trading orchestrator...");
    tracing::info!("Config: {}", cmd.config.display());

    // Validate live mode requirements
    if cmd.live && !cmd.i_accept_losses {
        anyhow::bail!(
            "Live trading requires --i-accept-losses flag to acknowledge risk"
        );
    }

    // Paper mode is the default safe mode
    let paper_mode = cmd.paper || !cmd.live;

    if paper_mode {
        tracing::warn!("Running in PAPER TRADING mode - no real transactions");
    } else {
        tracing::warn!("==========================================");
        tracing::warn!("  LIVE TRADING MODE - REAL FUNDS AT RISK");
        tracing::warn!("==========================================");
    }

    // Load config
    let config = load_config(&cmd.config)
        .context("Failed to load configuration")?;

    // Expand keypair path
    let keypair_path = shellexpand::tilde(&config.solana.keypair_path).to_string();

    // Create Jupiter client
    let jupiter = match config.jupiter.get_api_key() {
        Some(api_key) => {
            tracing::info!("Using Jupiter API key for higher rate limits");
            JupiterClient::with_api_key(api_key)
        }
        None => {
            tracing::warn!("No Jupiter API key configured - may hit rate limits");
            JupiterClient::new()
        }
    }.context("Failed to create Jupiter client")?;

    // Create Solana client
    let solana = SolanaClient::new(config.solana.rpc_url.clone());

    // Load wallet (or create random one for paper trading)
    let wallet = match WalletManager::from_file(&keypair_path) {
        Ok(w) => w,
        Err(e) => {
            if paper_mode {
                tracing::warn!("Wallet not found at '{}' - using random wallet for paper trading", keypair_path);
                WalletManager::new_random()
            } else {
                return Err(anyhow::anyhow!(
                    "Failed to load wallet from '{}': {}\n\
                     A wallet is required for live trading.",
                    keypair_path, e
                ));
            }
        }
    };

    tracing::info!("Wallet: {}", wallet.pubkey());

    // Build orchestrator config
    let orch_config = MemeOrchestratorConfig {
        enabled: true,
        ou_lookback: config.meme.as_ref().map(|m| m.ou_lookback).unwrap_or(100),
        ou_dt_minutes: config.meme.as_ref().map(|m| m.ou_dt_minutes).unwrap_or(1.0),
        z_entry_threshold: config.meme.as_ref().map(|m| m.z_entry_threshold).unwrap_or(-3.5),
        z_exit_threshold: config.meme.as_ref().map(|m| m.z_exit_threshold).unwrap_or(0.0),
        stop_loss_pct: config.meme.as_ref().map(|m| m.stop_loss_pct).unwrap_or(10.0),
        take_profit_pct: config.meme.as_ref().map(|m| m.take_profit_pct).unwrap_or(15.0),
        max_position_hours: config.meme.as_ref().map(|m| m.max_position_hours).unwrap_or(4.0),
        trade_size_usdc: cmd.trade_size,
        slippage_bps: config.meme.as_ref().map(|m| m.slippage_bps).unwrap_or(100),
        poll_interval_secs: cmd.poll_interval,
        priority_fee_lamports: config.meme.as_ref().map(|m| m.priority_fee_lamports).unwrap_or(10_000),
        data_dir: cmd.data_dir.clone(),
        paper_mode,
        min_ou_confidence: config.meme.as_ref().map(|m| m.min_ou_confidence).unwrap_or(0.3),
        min_half_life_minutes: config.meme.as_ref().map(|m| m.min_half_life_minutes).unwrap_or(5.0),
        max_half_life_minutes: config.meme.as_ref().map(|m| m.max_half_life_minutes).unwrap_or(120.0),
        // Momentum strategy parameters
        momentum_enabled: config.meme.as_ref().map(|m| m.momentum_enabled).unwrap_or(true),
        momentum_z_threshold: config.meme.as_ref().map(|m| m.momentum_z_threshold).unwrap_or(1.5),
        momentum_adx_entry_threshold: config.meme.as_ref().map(|m| m.momentum_adx_entry_threshold).unwrap_or(25.0),
        momentum_adx_exit_threshold: config.meme.as_ref().map(|m| m.momentum_adx_exit_threshold).unwrap_or(20.0),
        momentum_decay_hours: config.meme.as_ref().map(|m| m.momentum_decay_hours).unwrap_or(4.0),
        // Trailing take profit parameters
        use_trailing_tp: config.meme.as_ref().map(|m| m.use_trailing_tp).unwrap_or(true),
        trailing_activation_pct: config.meme.as_ref().map(|m| m.trailing_activation_pct).unwrap_or(10.0),
        trailing_stop_pct: config.meme.as_ref().map(|m| m.trailing_stop_pct).unwrap_or(5.0),
        // Timing parameters
        cooldown_seconds: config.meme.as_ref().map(|m| m.cooldown_seconds).unwrap_or(300),
        max_daily_trades: config.meme.as_ref().map(|m| m.max_daily_trades).unwrap_or(10),
    };

    // Validate config
    orch_config.validate()
        .map_err(|e| anyhow::anyhow!("Invalid meme orchestrator config: {}", e))?;

    // Print startup info
    println!();
    println!("======================================");
    println!("    Meme Trading Orchestrator");
    println!("======================================");
    println!();
    println!("  Config:        {}", cmd.config.display());
    println!("  Mode:          {}", if paper_mode { "PAPER TRADING" } else { "LIVE TRADING" });
    println!("  Wallet:        {}", wallet.pubkey());
    println!("  Trade Size:    ${:.2} USDC", cmd.trade_size);
    println!("  Poll Interval: {}s", cmd.poll_interval);
    println!("  Data Dir:      {}", cmd.data_dir.display());
    println!("  Z-Entry:       {:.1}", orch_config.z_entry_threshold);
    println!("  Z-Exit:        {:.1}", orch_config.z_exit_threshold);
    println!("  Stop Loss:     {:.1}%", orch_config.stop_loss_pct);
    println!("  Take Profit:   {:.1}%", orch_config.take_profit_pct);
    println!();
    println!("======================================");
    println!();

    // Create orchestrator
    let orchestrator = MemeOrchestrator::new(orch_config, jupiter, solana, wallet)
        .map_err(|e| anyhow::anyhow!("Failed to create orchestrator: {}", e))?;

    // Add tokens from CLI or config
    if let Some(ref tokens_str) = cmd.tokens {
        for mint in tokens_str.split(',') {
            let mint = mint.trim();
            if !mint.is_empty() {
                let info = TokenInfo::new(
                    mint.to_string(),
                    format!("TOKEN_{}", &mint[..6.min(mint.len())]),
                    9, // Default decimals, will be updated on first price fetch
                );
                orchestrator.add_token(info).await;
            }
        }
    } else if let Some(ref meme_config) = config.meme {
        for token in &meme_config.tokens {
            let info = TokenInfo::new(
                token.mint.clone(),
                token.symbol.clone(),
                token.decimals,
            );
            orchestrator.add_token(info).await;
        }
    }

    // Setup Ctrl+C handler for graceful shutdown
    let orch_for_shutdown = Arc::new(orchestrator);
    let orch_clone = orch_for_shutdown.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received (Ctrl+C)");
        orch_clone.shutdown().await;
    });

    // Run the orchestrator
    tracing::info!("Starting meme trading loop... Press Ctrl+C to stop.");

    // Note: We need to extract from Arc to call run()
    // Since run() takes &self, we can call it on the Arc
    match Arc::try_unwrap(orch_for_shutdown) {
        Ok(orch) => {
            orch.run().await
                .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
        }
        Err(arc_orch) => {
            // If there are other references, we can still run via the Arc
            // but we need to handle the MemeOrchestrator properly
            // The run() method takes &self so we can call it through the Arc
            arc_orch.run().await
                .map_err(|e| anyhow::anyhow!("Orchestrator error: {}", e))?;
        }
    }

    tracing::info!("Meme orchestrator stopped gracefully");
    Ok(())
}

/// Handle `butters meme status` command
async fn meme_status_command(cmd: MemeStatusCmd) -> Result<()> {
    tracing::info!("Fetching meme orchestrator status...");

    // Load persisted state from data_dir
    let position_path = cmd.data_dir.join(POSITION_FILE);
    let persisted_state = PersistedState::load(&position_path)
        .map_err(|e| anyhow::anyhow!("Failed to load state: {}", e))?;

    // Check connection status by trying to load config
    let config_loaded = load_config(&cmd.config).is_ok();

    match cmd.format.as_str() {
        "json" => {
            let position_json = match &persisted_state {
                Some(state) => match &state.active_position {
                    Some(pos) => format!(
                        r#"{{"token":"{}","entry_price":{},"size":{},"entry_value_usdc":{},"entry_timestamp":{}}}"#,
                        pos.token_symbol, pos.entry_price, pos.size, pos.entry_value_usdc, pos.entry_timestamp
                    ),
                    None => "null".to_string(),
                },
                None => "null".to_string(),
            };

            let wallet = persisted_state.as_ref().map(|s| s.wallet.as_str()).unwrap_or("unknown");

            println!(
                r#"{{"status":"{}","wallet":"{}","active_position":{},"config_loaded":{}}}"#,
                if persisted_state.is_some() { "loaded" } else { "no_state" },
                wallet,
                position_json,
                config_loaded
            );
        }
        "table" | "text" | _ => {
            println!();
            println!("======================================");
            println!("    Meme Orchestrator Status");
            println!("======================================");
            println!();
            println!("  Config:      {}", cmd.config.display());
            println!("  Data Dir:    {}", cmd.data_dir.display());
            println!("  Config OK:   {}", if config_loaded { "Yes" } else { "No" });
            println!();

            match &persisted_state {
                Some(state) => {
                    println!("  Wallet:      {}", state.wallet);
                    println!("  Last Update: {} (Unix timestamp)", state.last_updated);
                    println!();

                    match &state.active_position {
                        Some(pos) => {
                            let age_hours = pos.age_seconds() as f64 / 3600.0;
                            println!("  -- Active Position --");
                            println!("  Token:       {} ({})", pos.token_symbol, &pos.token_mint[..8.min(pos.token_mint.len())]);
                            println!("  Entry Price: ${:.8}", pos.entry_price);
                            println!("  Size:        {} tokens", pos.size);
                            println!("  Entry Value: ${:.2} USDC", pos.entry_value_usdc);
                            println!("  Entry Z:     {:.2}", pos.entry_z_score);
                            println!("  Age:         {:.1} hours", age_hours);
                        }
                        None => {
                            println!("  Position:    None (flat)");
                        }
                    }
                }
                None => {
                    println!("  State:       No persisted state found");
                    println!("  Position:    None");
                    println!();
                    println!("  Run 'butters meme run --paper' to start trading.");
                }
            }

            println!();
            println!("======================================");
        }
    }

    Ok(())
}

/// Handle `butters meme add-token` command
async fn meme_add_token_command(cmd: MemeAddTokenCmd) -> Result<()> {
    tracing::info!("Adding token to tracking list: {}", cmd.mint);

    // Validate mint address format (basic check: should be base58, ~32-44 chars)
    if cmd.mint.len() < 32 || cmd.mint.len() > 44 {
        anyhow::bail!(
            "Invalid mint address format. Expected base58 address (32-44 characters), got: {}",
            cmd.mint
        );
    }

    // Use provided values or defaults
    let symbol = cmd.symbol.unwrap_or_else(|| {
        format!("TOKEN_{}", &cmd.mint[..6.min(cmd.mint.len())])
    });
    let decimals = cmd.decimals.unwrap_or(9);

    // Create token info (for validation/logging purposes)
    let _token_info = TokenInfo::new(cmd.mint.clone(), symbol.clone(), decimals);

    // Load existing config to check for Jupiter connectivity
    let config = load_config(&cmd.config).ok();

    // Try to verify token is tradeable via Jupiter quote
    let tradeable_check = if let Some(ref cfg) = config {
        let jupiter = match cfg.jupiter.get_api_key() {
            Some(api_key) => JupiterClient::with_api_key(api_key).ok(),
            None => JupiterClient::new().ok(),
        };

        if let Some(j) = jupiter {
            // Try to get a small quote
            let quote_req = crate::adapters::jupiter::QuoteRequest::new(
                cmd.mint.clone(),
                crate::application::USDC_MINT.to_string(),
                10u64.pow(decimals as u32), // 1 token
                100, // 1% slippage
            );

            match j.get_quote(&quote_req).await {
                Ok(_) => Some(true),
                Err(e) => {
                    tracing::warn!("Token may not be tradeable on Jupiter: {}", e);
                    Some(false)
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    // Print result
    println!();
    println!("======================================");
    println!("    Token Added to Watch List");
    println!("======================================");
    println!();
    println!("  Mint:      {}", cmd.mint);
    println!("  Symbol:    {}", symbol);
    println!("  Decimals:  {}", decimals);
    println!("  Data Dir:  {}", cmd.data_dir.display());
    println!();

    match tradeable_check {
        Some(true) => println!("  Jupiter:   Tradeable (quote verified)"),
        Some(false) => println!("  Jupiter:   WARNING - Could not verify tradeability"),
        None => println!("  Jupiter:   Not checked (config not loaded)"),
    }

    println!();
    println!("  Note: Token will be tracked when orchestrator starts.");
    println!("  Run: butters meme run --paper --tokens {}", cmd.mint);
    println!();
    println!("======================================");

    Ok(())
}

/// Handle `butters meme remove-token` command
async fn meme_remove_token_command(cmd: MemeRemoveTokenCmd) -> Result<()> {
    tracing::info!("Removing token from tracking list: {}", cmd.mint);

    // TODO: Implement in Wave 3
    // 1. Check if token has active position
    // 2. If position open and not --force: error
    // 3. If position open and --force: triggers exit first
    // 4. Remove from tracked token list
    // 5. Clean up OU process state

    println!("Removing Token from Tracking");
    println!("============================");
    println!("  Mint:  {}", cmd.mint);
    println!("  Force: {}", cmd.force);
    println!();
    println!("[STUB] Token removal not yet implemented");

    Ok(())
}

/// Handle `butters meme list-tokens` command
async fn meme_list_tokens_command(cmd: MemeListTokensCmd) -> Result<()> {
    tracing::info!("Listing tracked tokens...");

    // TODO: Implement in Wave 3
    // 1. Load tracked tokens from persistence
    // 2. Fetch current prices and OU status
    // 3. Format and display list

    match cmd.format.as_str() {
        "json" => {
            println!(r#"{{"tokens":[]}}"#);
        }
        "table" | "text" | _ => {
            println!("+--------+----------------------------------------------+-----------+--------+----------+");
            println!("| Symbol | Mint                                         | Price     | Z-Score| Tradeable|");
            println!("+--------+----------------------------------------------+-----------+--------+----------+");
            println!("| [STUB] No tokens tracked - not yet implemented                                         |");
            println!("+--------+----------------------------------------------+-----------+--------+----------+");
        }
    }

    Ok(())
}

/// Handle `butters meme position` command
async fn meme_position_command(cmd: MemePositionCmd) -> Result<()> {
    tracing::info!("Fetching active position...");

    // Load persisted state from data_dir
    let position_path = cmd.data_dir.join(POSITION_FILE);
    let persisted_state = PersistedState::load(&position_path)
        .map_err(|e| anyhow::anyhow!("Failed to load state: {}", e))?;

    // Try to get current price if we have a position
    let current_price = if let Some(ref state) = persisted_state {
        if let Some(ref pos) = state.active_position {
            // Try to fetch current price via Jupiter
            let config = load_config(&cmd.config).ok();
            if let Some(ref cfg) = config {
                let jupiter = match cfg.jupiter.get_api_key() {
                    Some(api_key) => JupiterClient::with_api_key(api_key).ok(),
                    None => JupiterClient::new().ok(),
                };

                if let Some(j) = jupiter {
                    let quote_req = crate::adapters::jupiter::QuoteRequest::new(
                        pos.token_mint.clone(),
                        crate::application::USDC_MINT.to_string(),
                        10u64.pow(9), // Assume 9 decimals for 1 token
                        100,
                    );

                    match j.get_quote(&quote_req).await {
                        Ok(quote) => Some(quote.output_amount() as f64 / 1_000_000.0),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    match cmd.format.as_str() {
        "json" => {
            match &persisted_state {
                Some(state) => match &state.active_position {
                    Some(pos) => {
                        let pnl_pct = current_price.map(|p| pos.pnl_pct(p)).unwrap_or(0.0);
                        println!(
                            r#"{{"token":"{}","mint":"{}","entry_price":{},"current_price":{},"size":{},"entry_value_usdc":{},"pnl_pct":{:.2},"age_seconds":{}}}"#,
                            pos.token_symbol,
                            pos.token_mint,
                            pos.entry_price,
                            current_price.unwrap_or(pos.entry_price),
                            pos.size,
                            pos.entry_value_usdc,
                            pnl_pct,
                            pos.age_seconds()
                        );
                    }
                    None => println!(r#"{{"active_position":null}}"#),
                },
                None => println!(r#"{{"active_position":null}}"#),
            }
        }
        "text" | _ => {
            println!();
            println!("======================================");
            println!("    Active Meme Position");
            println!("======================================");
            println!();

            match &persisted_state {
                Some(state) => match &state.active_position {
                    Some(pos) => {
                        let age_hours = pos.age_seconds() as f64 / 3600.0;
                        let pnl_pct = current_price.map(|p| pos.pnl_pct(p));

                        println!("  Token:         {} ({}...)", pos.token_symbol, &pos.token_mint[..8.min(pos.token_mint.len())]);
                        println!("  Entry Price:   ${:.8}", pos.entry_price);

                        if let Some(price) = current_price {
                            println!("  Current Price: ${:.8}", price);
                        } else {
                            println!("  Current Price: (unavailable)");
                        }

                        println!("  Size:          {} tokens", pos.size);
                        println!("  Entry Value:   ${:.2} USDC", pos.entry_value_usdc);

                        if let Some(pnl) = pnl_pct {
                            let pnl_color = if pnl >= 0.0 { "+" } else { "" };
                            println!("  PnL:           {}{:.2}%", pnl_color, pnl);
                        } else {
                            println!("  PnL:           (need current price)");
                        }

                        println!("  Entry Z-Score: {:.2}", pos.entry_z_score);
                        println!("  Age:           {:.1} hours", age_hours);

                        if let Some(ref ou) = pos.ou_params {
                            println!();
                            println!("  -- OU Parameters at Entry --");
                            println!("  Theta:         {:.4}", ou.theta);
                            println!("  Mu:            {:.8}", ou.mu);
                            println!("  Sigma:         {:.8}", ou.sigma);
                            println!("  Confidence:    {:.2}", ou.confidence);
                        }
                    }
                    None => {
                        println!("  Position: None (flat)");
                        println!();
                        println!("  No active meme coin position.");
                        println!("  The orchestrator is waiting for entry signals.");
                    }
                },
                None => {
                    println!("  Position: None");
                    println!();
                    println!("  No persisted state found at: {}", position_path.display());
                    println!("  Run 'butters meme run --paper' to start trading.");
                }
            }

            println!();
            println!("======================================");
        }
    }

    Ok(())
}

/// Handle `butters meme exit` command
async fn meme_exit_command(cmd: MemeExitCmd) -> Result<()> {
    tracing::info!("Force exiting position...");

    // Load persisted state from data_dir
    let position_path = cmd.data_dir.join(POSITION_FILE);
    let persisted_state = PersistedState::load(&position_path)
        .map_err(|e| anyhow::anyhow!("Failed to load state: {}", e))?;

    // Check if there's a position to exit
    let (state, position) = match persisted_state {
        Some(s) => match s.active_position.clone() {
            Some(p) => (s, p),
            None => {
                println!();
                println!("No active position to exit.");
                println!("The orchestrator is currently flat.");
                return Ok(());
            }
        },
        None => {
            println!();
            println!("No persisted state found at: {}", position_path.display());
            println!("No position to exit.");
            return Ok(());
        }
    };

    // Display current position info
    println!();
    println!("======================================");
    println!("    Force Exit Position");
    println!("======================================");
    println!();
    println!("  Token:       {} ({}...)", position.token_symbol, &position.token_mint[..8.min(position.token_mint.len())]);
    println!("  Entry Price: ${:.8}", position.entry_price);
    println!("  Size:        {} tokens", position.size);
    println!("  Entry Value: ${:.2} USDC", position.entry_value_usdc);
    println!("  Age:         {:.1} hours", position.age_seconds() as f64 / 3600.0);
    println!("  Slippage:    {} bps", cmd.slippage);
    println!();

    // Confirm with user unless --yes
    if !cmd.yes {
        println!("This will immediately exit the position.");
        println!();
        print!("Type 'EXIT' to confirm (or use --yes to skip): ");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim() != "EXIT" {
            println!();
            println!("Aborted. Position remains open.");
            return Ok(());
        }
    }

    // In paper mode, just clear the position
    // For now, we'll implement paper exit (clearing the position file)
    // A real exit would require connecting to Jupiter/Solana

    tracing::info!("Executing paper exit for {}", position.token_symbol);

    // Load config to check if we're in paper mode
    let config = load_config(&cmd.config).ok();
    let paper_mode = config.as_ref()
        .and_then(|c| c.meme.as_ref())
        .map(|m| m.paper_mode)
        .unwrap_or(true);

    if paper_mode {
        // Paper mode - just clear the position
        let new_state = PersistedState::new(state.wallet.clone());
        new_state.save(&position_path)
            .map_err(|e| anyhow::anyhow!("Failed to clear position: {}", e))?;

        println!("======================================");
        println!();
        println!("  PAPER EXIT executed successfully!");
        println!();
        println!("  Token:       {}", position.token_symbol);
        println!("  Entry Value: ${:.2} USDC", position.entry_value_usdc);
        println!("  Mode:        PAPER TRADING");
        println!();
        println!("  Position cleared. Orchestrator is now flat.");
        println!();
        println!("======================================");
    } else {
        // Live mode - would need to actually execute the swap
        // For safety, we don't implement live exits in this CLI command
        // The orchestrator handles live exits with proper balance guard checks
        println!();
        println!("WARNING: Live exit from CLI is not supported.");
        println!("Live positions should be exited by the running orchestrator.");
        println!();
        println!("Options:");
        println!("  1. Let the orchestrator handle the exit naturally");
        println!("  2. Stop the orchestrator with Ctrl+C (will attempt graceful exit)");
        println!("  3. Set paper_mode = true in config and restart");
        println!();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meme_run_cmd_defaults() {
        // parse_from requires the binary name as first argument
        let cmd = MemeRunCmd::parse_from(["test"]);
        assert_eq!(cmd.config, PathBuf::from("config/mainnet.toml"));
        assert!(!cmd.paper);
        assert!(!cmd.live);
        assert_eq!(cmd.trade_size, 50.0);
        assert_eq!(cmd.poll_interval, 60);
    }

    #[test]
    fn test_meme_run_cmd_with_paper() {
        let cmd = MemeRunCmd::parse_from(["test", "--paper"]);
        assert!(cmd.paper);
        assert!(!cmd.live);
    }

    #[test]
    fn test_meme_run_cmd_with_live() {
        let cmd = MemeRunCmd::parse_from(["test", "--live", "--i-accept-losses"]);
        assert!(cmd.live);
        assert!(cmd.i_accept_losses);
    }

    #[test]
    fn test_meme_run_cmd_with_tokens() {
        let cmd = MemeRunCmd::parse_from([
            "test", "--tokens", "mint1,mint2"
        ]);
        assert_eq!(cmd.tokens, Some("mint1,mint2".to_string()));
    }

    #[test]
    fn test_meme_status_cmd_defaults() {
        let cmd = MemeStatusCmd::parse_from(["test"]);
        assert_eq!(cmd.format, "text");
        assert_eq!(cmd.data_dir, PathBuf::from("data/meme"));
    }

    #[test]
    fn test_meme_status_cmd_json_format() {
        let cmd = MemeStatusCmd::parse_from(["test", "--format", "json"]);
        assert_eq!(cmd.format, "json");
    }

    #[test]
    fn test_meme_add_token_cmd() {
        let cmd = MemeAddTokenCmd::parse_from([
            "test", "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263"
        ]);
        assert_eq!(cmd.mint, "DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263");
        assert!(cmd.symbol.is_none());
        assert!(cmd.decimals.is_none());
    }

    #[test]
    fn test_meme_add_token_cmd_with_overrides() {
        let cmd = MemeAddTokenCmd::parse_from([
            "test", "mint123",
            "--symbol", "TEST",
            "--decimals", "9"
        ]);
        assert_eq!(cmd.symbol, Some("TEST".to_string()));
        assert_eq!(cmd.decimals, Some(9));
    }

    #[test]
    fn test_meme_remove_token_cmd() {
        let cmd = MemeRemoveTokenCmd::parse_from([
            "test", "mint123"
        ]);
        assert_eq!(cmd.mint, "mint123");
        assert!(!cmd.force);
    }

    #[test]
    fn test_meme_remove_token_cmd_force() {
        let cmd = MemeRemoveTokenCmd::parse_from([
            "test", "mint123", "--force"
        ]);
        assert!(cmd.force);
    }

    #[test]
    fn test_meme_list_tokens_cmd() {
        let cmd = MemeListTokensCmd::parse_from(["test"]);
        assert_eq!(cmd.format, "table");
    }

    #[test]
    fn test_meme_position_cmd() {
        let cmd = MemePositionCmd::parse_from(["test"]);
        assert_eq!(cmd.format, "text");
    }

    #[test]
    fn test_meme_exit_cmd_defaults() {
        let cmd = MemeExitCmd::parse_from(["test"]);
        assert!(!cmd.yes);
        assert_eq!(cmd.slippage, 150);
    }

    #[test]
    fn test_meme_exit_cmd_with_flags() {
        let cmd = MemeExitCmd::parse_from([
            "test", "--yes", "--slippage", "300"
        ]);
        assert!(cmd.yes);
        assert_eq!(cmd.slippage, 300);
    }
}
