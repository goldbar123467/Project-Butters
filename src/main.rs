#![allow(dead_code, unused_imports, unused_variables)]
//! Butters - Jupiter Mean Reversion DEX Trading Bot
//!
//! A conservative mean reversion trading strategy for Solana via Jupiter aggregator.

mod domain;
mod ports;
mod strategy;
mod adapters;
mod config;
mod application;
mod meme;

use anyhow::{Result, Context, bail};
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::adapters::cli::{CliApp, Command, RunCmd, StatusCmd, QuoteCmd, SwapCmd, BacktestCmd, ResumeCmd, MemeCmd};
use crate::adapters::jito::{JitoBundleClient, JitoConfig, JitoExecutionAdapter};
use crate::adapters::jupiter::JupiterClient;
use crate::adapters::solana::{SolanaClient, WalletManager};
use crate::application::TradingOrchestrator;
use crate::config::load_config;
use crate::strategy::StrategyConfig;
use crate::ports::execution::{ExecutionPort, SwapQuoteRequest, ExecuteSwapRequest};

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
        Command::Resume(cmd) => resume_command(cmd).await,
        Command::Meme(cmd) => meme_command(cmd).await,
    }
}

fn init_logging(verbose: bool, debug: bool) -> Result<()> {
    let filter = if debug {
        EnvFilter::new("debug")
    } else if verbose {
        EnvFilter::new("trace")
    } else {
        EnvFilter::new("info")
    };

    fmt().with_env_filter(filter).init();
    Ok(())
}

/// Preflight safety checks for live mainnet trading
///
/// These checks help prevent accidental loss of funds by ensuring:
/// 1. Keypair file has secure permissions (Unix: 600 or stricter)
/// 2. Not using devnet RPC for live trading
/// 3. Jito MEV protection is enabled (warning if disabled)
async fn preflight_checks(config: &config::Config, keypair_path: &str) -> Result<()> {
    tracing::info!("Running preflight safety checks for live trading...");

    // 1. Check keypair file permissions (Unix only)
    #[cfg(unix)]
    {
        let metadata = std::fs::metadata(keypair_path)
            .context(format!("Cannot access keypair file: {}", keypair_path))?;
        let mode = metadata.permissions().mode();
        // Check if group or other permissions are set (should be 0)
        if mode & 0o077 != 0 {
            bail!(
                "SECURITY ERROR: Keypair file has unsafe permissions: {:o}\n\
                 \n\
                 The keypair file '{}' is readable by other users.\n\
                 For live trading, this file must have permissions 600 or stricter.\n\
                 \n\
                 Fix with: chmod 600 {}",
                mode & 0o777,
                keypair_path,
                keypair_path
            );
        }
        tracing::info!("Keypair permissions: {:o} (OK)", mode & 0o777);
    }

    // 2. Check not devnet
    if config.solana.rpc_url.contains("devnet") {
        bail!(
            "CONFIGURATION ERROR: Cannot use --live with devnet RPC.\n\
             \n\
             Your config specifies a devnet RPC URL: {}\n\
             Live trading requires a mainnet RPC endpoint.\n\
             \n\
             Update your config.toml to use a mainnet RPC URL.",
            config.solana.rpc_url
        );
    }
    tracing::info!("RPC endpoint: {} (mainnet)", config.solana.rpc_url);

    // 3. Warn if Jito MEV protection is disabled
    if !config.jito.enabled {
        tracing::warn!("==================================================");
        tracing::warn!("WARNING: Jito MEV protection is DISABLED on mainnet!");
        tracing::warn!("Your trades may be frontrun by MEV bots.");
        tracing::warn!("Consider enabling Jito bundles in your config.toml:");
        tracing::warn!("  [jito]");
        tracing::warn!("  enabled = true");
        tracing::warn!("==================================================");
    } else {
        tracing::info!("Jito MEV protection: ENABLED (region: {})", config.jito.region);
    }

    // 4. Log minimum balance recommendation
    tracing::info!("Recommended minimum balance: 0.05 SOL (50,000,000 lamports)");
    tracing::info!("This covers transaction fees and Jito tips for multiple trades.");

    tracing::info!("Preflight checks PASSED for live trading");
    Ok(())
}

async fn run_command(cmd: RunCmd) -> Result<()> {
    tracing::info!("Starting Butters trading bot...");

    // Load config
    let config = load_config(&cmd.config)
        .context("Failed to load configuration")?;

    // Expand keypair path (handles ~ for home directory)
    let keypair_path = shellexpand::tilde(&config.solana.keypair_path).to_string();

    // Check for conflicting flags
    if cmd.live && cmd.paper {
        bail!(
            "Conflicting flags: --live and --paper cannot be used together.\n\
             Use --paper for simulation or --live for real trading."
        );
    }

    // Preflight checks for live trading
    if cmd.live {
        if !cmd.i_accept_losses {
            bail!(
                "LIVE TRADING SAFEGUARD\n\
                 \n\
                 Live trading on mainnet involves real financial risk.\n\
                 You could lose some or all of your funds.\n\
                 \n\
                 To proceed with live trading, you must acknowledge this risk:\n\
                 \n\
                 ./target/release/butters run --live --i-accept-losses\n\
                 \n\
                 For simulation without risk, use: --paper"
            );
        }

        tracing::warn!("==========================================");
        tracing::warn!("  LIVE TRADING MODE - REAL FUNDS AT RISK");
        tracing::warn!("==========================================");

        // Run preflight safety checks
        preflight_checks(&config, &keypair_path).await?;
    }

    // Build components - use API key from config/env for higher rate limits
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
    let solana = SolanaClient::new(config.solana.rpc_url.clone());

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

    // Create Jito client if enabled
    let jito_execution = if config.jito.enabled {
        let jito_config = JitoConfig::mainnet(&config.jito.region)
            .with_tip(config.jito.tip_lamports);
        
        let jito_config = if let Some(ref token) = config.jito.api_token {
            jito_config.with_api_token(token.clone())
        } else {
            jito_config
        };
        
        let jito = JitoBundleClient::with_config(jito_config)
            .context("Failed to create Jito client")?;
        
        tracing::info!("Jito MEV protection enabled (region: {})", config.jito.region);
        Some(JitoExecutionAdapter::new(jupiter.clone(), jito, wallet.pubkey()))
    } else {
        tracing::info!("Jito MEV protection disabled");
        None
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
        config.risk.trade_size_sol,
        config.jupiter.max_priority_fee_lamports,
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

    if jito_execution.is_some() {
        tracing::info!("MEV protection: ENABLED via Jito bundles");
    } else {
        tracing::info!("MEV protection: DISABLED (direct Jupiter swaps)");
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
    let jupiter = match config.jupiter.get_api_key() {
        Some(api_key) => JupiterClient::with_api_key(api_key)?,
        None => JupiterClient::new()?,
    };

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
    // Load config
    let config = load_config(&cmd.config)
        .context("Failed to load configuration")?;

    // Create Jupiter client with API key for higher rate limits
    let jupiter = match config.jupiter.get_api_key() {
        Some(api_key) => JupiterClient::with_api_key(api_key),
        None => JupiterClient::new(),
    }.context("Failed to create Jupiter client")?;

    // Load wallet
    let keypair_path = shellexpand::tilde(&config.solana.keypair_path).to_string();
    let wallet = load_wallet_with_context(&keypair_path, false)?;

    // Resolve tokens
    let (input_mint, output_mint) = match (cmd.input_token.as_str(), cmd.output_token.as_str()) {
        ("SOL", "USDC") => (config.tokens.base_mint.clone(), config.tokens.quote_mint.clone()),
        ("USDC", "SOL") => (config.tokens.quote_mint.clone(), config.tokens.base_mint.clone()),
        _ => anyhow::bail!("Unsupported token pair. Use 'SOL' or 'USDC'"),
    };

    let amount = (cmd.amount * 1e9) as u64; // Convert to lamports

    // Get quote from Jupiter
    tracing::info!("Fetching quote for {} {} -> {}", cmd.amount, cmd.input_token, cmd.output_token);
    let quote_req = crate::adapters::jupiter::QuoteRequest::new(
        input_mint.clone(), output_mint.clone(), amount, cmd.slippage
    );
    
    let quote = jupiter.get_quote(&quote_req).await
        .context("Failed to get quote")?;

    let output_amount = quote.output_amount() as f64 / 1e6;
    let price_impact = quote.price_impact();

    println!("\nQuote:");
    println!("  Input:  {} {}", cmd.amount, cmd.input_token);
    println!("  Output: {:.6} {}", output_amount, cmd.output_token);
    println!("  Price impact: {:.2}%", price_impact);
    println!("  Slippage: {} bps", cmd.slippage);

    if config.jito.enabled {
        println!("  MEV protection: ENABLED (Jito bundles)");
        println!("  Tip: {} lamports", config.jito.tip_lamports);
    } else {
        println!("  MEV protection: DISABLED");
    }

    // Confirm with user
    if !cmd.yes {
        println!("\nExecute this swap? [y/N]: ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Swap cancelled");
            return Ok(());
        }
    }

    // Execute swap with optional Jito protection
    tracing::info!("Executing swap...");
    
    if config.jito.enabled {
        // Use Jito MEV-protected execution
        let jito_config = JitoConfig::mainnet(&config.jito.region)
            .with_tip(config.jito.tip_lamports);
        
        let jito_config = if let Some(ref token) = config.jito.api_token {
            jito_config.with_api_token(token.clone())
        } else {
            jito_config
        };
        
        let jito = JitoBundleClient::with_config(jito_config)
            .context("Failed to create Jito client")?;
        
        tracing::info!("Using Jito MEV protection (region: {})", config.jito.region);
        let jito_adapter = JitoExecutionAdapter::new(jupiter, jito, wallet.pubkey());
        
        // Use ExecutionPort trait methods
        let swap_quote_req = SwapQuoteRequest {
            input_mint,
            output_mint,
            amount,
            slippage_bps: cmd.slippage,
            platform_fee_bps: None,
        };
        
        let quote_response = jito_adapter.get_swap_quote(swap_quote_req).await
            .context("Failed to get swap quote")?;
        
        let execute_req = ExecuteSwapRequest {
            quote_response,
            user_public_key: wallet.public_key().to_string(),
            prioritization_fee_lamports: Some(config.jito.tip_lamports),
            dynamic_compute_unit_limit: Some(true),
        };
        
        let response = jito_adapter.execute_swap(execute_req).await
            .context("Failed to execute Jito-protected swap")?;
        
        println!("\nSwap executed successfully!");
        println!("Bundle ID: {}", response.signature);
        println!("Status: {}", response.status);
    } else {
        // Direct Jupiter swap (build and execute)
        tracing::info!("Jito MEV protection disabled - using direct Jupiter swap");
        
        let swap_request = crate::adapters::jupiter::SwapRequest {
            user_public_key: wallet.public_key().to_string(),
            quote_response: serde_json::to_value(&quote)
                .context("Failed to serialize quote")?,
            prioritization_fee_lamports: None,
            dynamic_compute_unit_limit: true,
        };
        
        let swap_response = jupiter.get_swap_transaction(&swap_request).await
            .context("Failed to get swap transaction")?;
        
        println!("\nSwap transaction built successfully!");
        println!("Note: Transaction needs to be signed and submitted manually");
        println!("Transaction: {}", &swap_response.swap_transaction[..50]);
    }

    Ok(())
}

async fn backtest_command(_cmd: BacktestCmd) -> Result<()> {
    println!("Backtest command not yet implemented");
    Ok(())
}

async fn meme_command(cmd: MemeCmd) -> Result<()> {
    // Delegate to the meme module's execute function
    crate::meme::execute_meme_command(cmd).await
}

async fn resume_command(cmd: ResumeCmd) -> Result<()> {
    use crate::domain::GuardStatus;
    use std::io::{self, Write};

    tracing::info!("Checking BalanceGuard status...");

    // Load current status from file
    let status = match GuardStatus::load(&cmd.data_dir)
        .context("Failed to read guard status file")?
    {
        Some(s) => s,
        None => {
            println!("No BalanceGuard status file found at: {}", cmd.data_dir.display());
            println!("This could mean:");
            println!("  - Trading has not been halted");
            println!("  - The bot has never run with this data directory");
            println!("  - The status file was already cleared");
            println!();
            println!("If you're sure trading should be resumed, you can create a resume signal:");
            println!("  The bot will check for this on next startup.");

            if cmd.force {
                println!();
                println!("Creating resume signal file...");
                let resume_status = GuardStatus::resumed("unknown");
                resume_status.save(&cmd.data_dir)
                    .context("Failed to save resume signal")?;
                println!("Resume signal created. The bot will start unhalted on next run.");
            }
            return Ok(());
        }
    };

    // Display current status
    println!();
    println!("======================================");
    println!("    BalanceGuard Status Report");
    println!("======================================");
    println!();
    println!("  Wallet:          {}", status.wallet);
    println!("  Status:          {}", if status.is_halted { "HALTED" } else { "ACTIVE" });
    println!("  Cumulative Loss: {} lamports ({:.6} SOL)",
        status.cumulative_unexplained,
        status.cumulative_unexplained as f64 / 1e9
    );
    println!("  Last Updated:    {}", format_timestamp(status.last_updated));

    if let Some(ref reason) = status.halt_reason {
        println!("  Halt Reason:     {}", reason);
    }

    if !status.recent_violations.is_empty() {
        println!();
        println!("  Recent Violations:");
        for (i, v) in status.recent_violations.iter().take(5).enumerate() {
            println!("    {}. [{}] Expected: {} lamports, Actual: {} (Diff: {})",
                i + 1,
                format_timestamp(v.timestamp),
                v.expected,
                v.actual,
                v.diff
            );
            println!("       Reason: {}", v.reason);
        }
    }
    println!();
    println!("======================================");

    // If not halted, nothing to do
    if !status.is_halted {
        println!();
        println!("Trading is NOT halted. No action needed.");
        return Ok(());
    }

    // Confirm resume
    if !cmd.force {
        println!();
        println!("WARNING: Resuming trading will clear the halt state.");
        println!("Make sure you have investigated the balance anomaly before proceeding.");
        println!();
        print!("Type 'RESUME' to confirm (or use --force to skip this prompt): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if input.trim() != "RESUME" {
            println!("Aborted. Trading remains halted.");
            return Ok(());
        }
    }

    // Create resumed status
    let resumed_status = if cmd.reset_cumulative {
        tracing::info!("Resetting cumulative loss counter");
        GuardStatus::resumed(&status.wallet)
    } else {
        // Preserve wallet but clear halt
        GuardStatus {
            is_halted: false,
            cumulative_unexplained: status.cumulative_unexplained,
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            halt_reason: None,
            recent_violations: vec![], // Clear violations after resume
            wallet: status.wallet,
        }
    };

    // Save resumed status
    resumed_status.save(&cmd.data_dir)
        .context("Failed to save resumed status")?;

    println!();
    println!("Trading RESUMED successfully.");
    println!("The bot will check this status file on next startup and begin trading.");
    if cmd.reset_cumulative {
        println!("Cumulative loss counter has been reset to 0.");
    } else {
        println!("Note: Cumulative loss counter preserved at {} lamports.",
            resumed_status.cumulative_unexplained);
        println!("Use --reset-cumulative to also reset the loss counter.");
    }

    Ok(())
}

/// Format a Unix timestamp for display
fn format_timestamp(ts: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let datetime = UNIX_EPOCH + Duration::from_secs(ts);
    match datetime.duration_since(UNIX_EPOCH) {
        Ok(_) => {
            // Simple formatting without chrono
            let secs_since_epoch = ts;
            let days = secs_since_epoch / 86400;
            let remaining = secs_since_epoch % 86400;
            let hours = remaining / 3600;
            let minutes = (remaining % 3600) / 60;
            let seconds = remaining % 60;

            // Approximate date calculation (good enough for display)
            let years = 1970 + days / 365;
            let day_of_year = days % 365;

            format!("{}-day-{} {:02}:{:02}:{:02} UTC",
                years, day_of_year, hours, minutes, seconds)
        }
        Err(_) => format!("timestamp: {}", ts),
    }
}
