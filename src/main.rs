//! Butters - Jupiter Mean Reversion DEX Trading Bot
//!
//! A conservative mean reversion trading strategy for Solana via Jupiter aggregator.

mod domain;
mod ports;
mod strategy;
mod adapters;
mod config;
mod application;

use anyhow::{Result, Context, bail};
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};
use std::path::Path;

use crate::adapters::cli::{CliApp, Command, RunCmd, StatusCmd, QuoteCmd, SwapCmd, BacktestCmd};
use crate::adapters::jupiter::JupiterClient;
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::application::TradingOrchestrator;
use crate::config::load_config;
use crate::strategy::StrategyConfig;

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if it exists (secrets go here, not in config.toml)
    dotenvy::dotenv().ok();

    let app = CliApp::parse();
    init_logging(app.verbose, app.debug)?;

    match app.command {
        Command::Run(cmd) => run_command(cmd).await,
        Command::Status(cmd) => status_command(cmd).await,
        Command::Quote(cmd) => quote_command(cmd).await,
        Command::Swap(cmd) => swap_command(cmd).await,
        Command::Backtest(cmd) => backtest_command(cmd).await,
    }
}

fn init_logging(verbose: bool, debug: bool) -> Result<()> {
    let filter = if debug {
        EnvFilter::new("debug")
    } else if verbose {
        EnvFilter::new("info")
    } else {
        EnvFilter::new("warn")
    };

    fmt().with_env_filter(filter).init();
    Ok(())
}

async fn run_command(cmd: RunCmd) -> Result<()> {
    tracing::info!("Starting Butters trading bot...");

    // Load config
    let config = load_config(&cmd.config)
        .context("Failed to load configuration")?;

    // Build components
    let jupiter = JupiterClient::new()
        .context("Failed to create Jupiter client")?;
    let solana = SolanaClient::new(config.solana.rpc_url.clone());

    // Expand keypair path (handles ~ for home directory)
    let keypair_path = shellexpand::tilde(&config.solana.keypair_path).to_string();

    // Load wallet with improved error handling
    let wallet = match load_wallet_with_context(&keypair_path, cmd.paper) {
        Ok(w) => w,
        Err(e) => {
            if cmd.paper {
                // In paper mode, create a random wallet and warn the user
                tracing::warn!("Wallet not found at '{}' - using random wallet for paper trading", keypair_path);
                tracing::warn!("To create a real wallet, run: solana-keygen new --outfile {}", keypair_path);
                WalletManager::new_random()
            } else {
                // In live mode, wallet is required - return helpful error
                return Err(e);
            }
        }
    };

    // Convert config to strategy config
    let strategy_config = StrategyConfig::from(&config);

    // Create orchestrator
    let orchestrator = TradingOrchestrator::new(
        strategy_config,
        jupiter,
        solana,
        wallet,
        config.tokens.base_mint.clone(),
        config.tokens.quote_mint.clone(),
        config.jupiter.slippage_bps,
        cmd.paper,
    ).context("Failed to create orchestrator")?;

    // Setup Ctrl+C handler
    let orch = orchestrator.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Shutdown signal received");
        orch.stop().await;
    });

    // Run
    if cmd.paper {
        tracing::warn!("PAPER TRADING MODE - no real transactions");
    }

    orchestrator.run().await?;
    tracing::info!("Butters stopped");
    Ok(())
}

async fn status_command(cmd: StatusCmd) -> Result<()> {
    let config = load_config(&cmd.config)?;
    let solana = SolanaClient::new(config.solana.rpc_url.clone());

    // Expand keypair path
    let keypair_path = shellexpand::tilde(&config.solana.keypair_path).to_string();
    let wallet = load_wallet_with_context(&keypair_path, false)?;

    let balance = solana.get_balance(&wallet.public_key()).await
        .context("Failed to get balance")?;

    println!("Wallet: {}", wallet.public_key());
    println!("Balance: {} lamports ({:.4} SOL)", balance, balance as f64 / 1e9);

    Ok(())
}

/// Load wallet with helpful error messages
fn load_wallet_with_context(keypair_path: &str, is_paper_mode: bool) -> Result<WalletManager> {
    let path = Path::new(keypair_path);

    // Check if file exists first for a clearer error message
    if !path.exists() {
        let mode_hint = if is_paper_mode {
            "In paper mode, a random wallet will be used instead."
        } else {
            "A wallet is required for live trading."
        };

        bail!(
            "Wallet file not found: {}\n\n\
             {}\n\n\
             To create a new wallet, run:\n  \
             solana-keygen new --outfile {}\n\n\
             Or if you have an existing wallet, update 'keypair_path' in your config.toml",
            keypair_path,
            mode_hint,
            keypair_path
        );
    }

    // Check if file is readable
    if let Err(e) = std::fs::metadata(path) {
        bail!(
            "Cannot access wallet file '{}': {}\n\n\
             Check file permissions and ensure the path is correct.",
            keypair_path,
            e
        );
    }

    // Try to load the wallet
    WalletManager::from_file(keypair_path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load wallet from '{}': {}\n\n\
             The file exists but may be corrupted or in the wrong format.\n\
             Expected format: JSON array of bytes (e.g., [1,2,3,...])\n\n\
             To create a new wallet, run:\n  \
             solana-keygen new --outfile {}",
            keypair_path,
            e,
            keypair_path
        )
    })
}

async fn quote_command(cmd: QuoteCmd) -> Result<()> {
    let config = load_config(&cmd.config)?;
    let jupiter = JupiterClient::new()?;

    // Resolve tokens
    let (input_mint, output_mint) = match (cmd.input_token.as_str(), cmd.output_token.as_str()) {
        ("SOL", "USDC") => (config.tokens.base_mint.clone(), config.tokens.quote_mint.clone()),
        ("USDC", "SOL") => (config.tokens.quote_mint.clone(), config.tokens.base_mint.clone()),
        _ => anyhow::bail!("Unsupported token pair"),
    };

    let amount = (cmd.amount * 1e9) as u64; // Convert to lamports
    let quote_req = crate::adapters::jupiter::QuoteRequest::new(
        input_mint, output_mint, amount, cmd.slippage
    );

    let quote = jupiter.get_quote(&quote_req).await
        .context("Failed to get quote")?;

    println!("Quote: {} {} -> {} {}",
        cmd.amount, cmd.input_token,
        quote.output_amount() as f64 / 1e6, cmd.output_token);
    println!("Price impact: {}%", quote.price_impact());

    Ok(())
}

async fn swap_command(cmd: SwapCmd) -> Result<()> {
    println!("Swap command not yet implemented");
    Ok(())
}

async fn backtest_command(cmd: BacktestCmd) -> Result<()> {
    println!("Backtest command not yet implemented");
    Ok(())
}
