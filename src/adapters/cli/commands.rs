//! CLI Command Handlers
//!
//! Implementation of all CLI commands for the Butters trading bot.

use clap::{Parser, Subcommand};
use anyhow::Result;
use std::path::PathBuf;

/// Butters - Mean Reversion DEX Trading Bot for Solana/Jupiter
#[derive(Parser, Debug)]
#[command(
    name = "butters",
    version = env!("CARGO_PKG_VERSION"),
    author = env!("CARGO_PKG_AUTHORS"),
    about = "Mean Reversion DEX Trading Bot for Solana/Jupiter",
    long_about = "Butters executes conservative mean reversion trades on Solana using \
                  Jupiter aggregator with z-score statistical gating for optimal entry points."
)]
pub struct CliApp {
    /// The command to execute
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Enable debug logging
    #[arg(long, global = true)]
    pub debug: bool,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Start the trading loop
    Run(RunCmd),

    /// Check bot status and portfolio
    Status(StatusCmd),

    /// Get a quote for a token swap
    Quote(QuoteCmd),

    /// Execute a token swap
    Swap(SwapCmd),

    /// Run backtesting on historical data
    Backtest(BacktestCmd),

    /// Resume trading after BalanceGuard halt
    Resume(ResumeCmd),
}

/// Start trading loop
#[derive(Parser, Debug)]
pub struct RunCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Run in paper trading mode (no real transactions)
    #[arg(short, long)]
    pub paper: bool,

    /// Enable live mainnet trading (requires --i-accept-losses)
    #[arg(long, help = "Enable live mainnet trading")]
    pub live: bool,

    /// Acknowledge risk of financial loss (required for --live)
    #[arg(long, help = "Acknowledge risk of financial loss")]
    pub i_accept_losses: bool,

    /// Override RPC URL
    #[arg(long, value_name = "URL")]
    pub rpc_url: Option<String>,

    /// Override keypair path
    #[arg(long, value_name = "FILE")]
    pub keypair: Option<PathBuf>,
}

/// Check bot status
#[derive(Parser, Debug)]
pub struct StatusCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Show detailed portfolio breakdown
    #[arg(short, long)]
    pub detailed: bool,

    /// Output format (text, json, table)
    #[arg(short, long, value_name = "FORMAT", default_value = "text")]
    pub format: String,
}

/// Get swap quote
#[derive(Parser, Debug)]
pub struct QuoteCmd {
    /// Input token symbol (e.g., SOL)
    #[arg(value_name = "INPUT")]
    pub input_token: String,

    /// Output token symbol (e.g., USDC)
    #[arg(value_name = "OUTPUT")]
    pub output_token: String,

    /// Amount to swap
    #[arg(value_name = "AMOUNT")]
    pub amount: f64,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Slippage tolerance in basis points (default: 50 = 0.5%)
    #[arg(long, value_name = "BPS", default_value = "50")]
    pub slippage: u16,

    /// Only use direct routes (no multi-hop)
    #[arg(long)]
    pub direct_only: bool,
}

/// Execute swap
#[derive(Parser, Debug)]
pub struct SwapCmd {
    /// Input token symbol (e.g., SOL)
    #[arg(value_name = "INPUT")]
    pub input_token: String,

    /// Output token symbol (e.g., USDC)
    #[arg(value_name = "OUTPUT")]
    pub output_token: String,

    /// Amount to swap
    #[arg(value_name = "AMOUNT")]
    pub amount: f64,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Slippage tolerance in basis points (default: 50 = 0.5%)
    #[arg(long, value_name = "BPS", default_value = "50")]
    pub slippage: u16,

    /// Confirm swap without prompting
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Only use direct routes (no multi-hop)
    #[arg(long)]
    pub direct_only: bool,

    /// Simulate swap without executing
    #[arg(long)]
    pub dry_run: bool,
}

/// Run backtesting
#[derive(Parser, Debug)]
pub struct BacktestCmd {
    /// Trading pair (e.g., SOL/USDC)
    #[arg(short, long, value_name = "PAIR")]
    pub pair: String,

    /// Number of days to backtest
    #[arg(short, long, value_name = "DAYS", default_value = "30")]
    pub days: u32,

    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Starting capital for backtest
    #[arg(long, value_name = "AMOUNT", default_value = "10000")]
    pub capital: f64,

    /// Override z-score threshold
    #[arg(long, value_name = "THRESHOLD")]
    pub z_threshold: Option<f64>,

    /// Override lookback period
    #[arg(long, value_name = "PERIODS")]
    pub lookback: Option<usize>,

    /// Output detailed trade log
    #[arg(short, long)]
    pub verbose: bool,

    /// Export results to CSV
    #[arg(long, value_name = "FILE")]
    pub export_csv: Option<PathBuf>,

    /// Export results to JSON
    #[arg(long, value_name = "FILE")]
    pub export_json: Option<PathBuf>,
}

/// Resume trading after BalanceGuard halt
#[derive(Parser, Debug)]
pub struct ResumeCmd {
    /// Path to configuration file
    #[arg(short, long, value_name = "FILE", default_value = "config/mainnet.toml")]
    pub config: PathBuf,

    /// Force resume without confirmation (bypass safety prompt)
    #[arg(long)]
    pub force: bool,

    /// Reset cumulative loss counter in addition to resuming
    #[arg(long)]
    pub reset_cumulative: bool,

    /// Path to data directory (for status file)
    #[arg(long, value_name = "DIR", default_value = "data")]
    pub data_dir: PathBuf,
}

/// Execute the CLI command
pub async fn execute(app: CliApp) -> Result<()> {
    // Initialize logging based on flags
    init_logging(app.verbose, app.debug)?;

    match app.command {
        Command::Run(cmd) => run_command(cmd).await,
        Command::Status(cmd) => status_command(cmd).await,
        Command::Quote(cmd) => quote_command(cmd).await,
        Command::Swap(cmd) => swap_command(cmd).await,
        Command::Backtest(cmd) => backtest_command(cmd).await,
        Command::Resume(cmd) => resume_command(cmd).await,
    }
}

/// Initialize logging system
fn init_logging(verbose: bool, debug: bool) -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = if debug {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"))
    } else if verbose {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    Ok(())
}

/// Handle run command
async fn run_command(cmd: RunCmd) -> Result<()> {
    tracing::info!("Starting Butters trading bot...");
    tracing::info!("Config: {}", cmd.config.display());

    if cmd.paper {
        tracing::warn!("Running in PAPER TRADING mode - no real transactions");
    }

    // TODO: Implement trading loop
    // 1. Load config from file
    // 2. Initialize Solana/Jupiter clients
    // 3. Initialize strategy
    // 4. Start trading loop

    println!("✓ Trading loop started (not yet implemented)");
    println!("  Config: {}", cmd.config.display());
    println!("  Mode: {}", if cmd.paper { "Paper Trading" } else { "Live Trading" });

    if let Some(ref rpc_url) = cmd.rpc_url {
        println!("  RPC: {}", rpc_url);
    }

    Ok(())
}

/// Handle status command
async fn status_command(cmd: StatusCmd) -> Result<()> {
    tracing::info!("Fetching bot status...");

    // TODO: Implement status check
    // 1. Load config
    // 2. Connect to Solana
    // 3. Fetch portfolio balances
    // 4. Display active positions

    match cmd.format.as_str() {
        "json" => {
            println!("{{\"status\":\"not_implemented\"}}");
        }
        "table" | "text" | _ => {
            println!("┌─────────────────────────────────────┐");
            println!("│  Butters Trading Bot - Status       │");
            println!("├─────────────────────────────────────┤");
            println!("│  Status: Not Implemented            │");
            println!("│  Config: {}  │", cmd.config.display());
            println!("└─────────────────────────────────────┘");
        }
    }

    if cmd.detailed {
        println!("\nDetailed status will show:");
        println!("  - Portfolio breakdown");
        println!("  - Active positions");
        println!("  - Daily P&L");
        println!("  - Risk metrics");
    }

    Ok(())
}

/// Handle quote command
async fn quote_command(cmd: QuoteCmd) -> Result<()> {
    tracing::info!("Fetching quote: {} -> {}", cmd.input_token, cmd.output_token);

    // TODO: Implement quote fetching
    // 1. Load config to get token addresses
    // 2. Initialize Jupiter client
    // 3. Fetch quote
    // 4. Display results

    println!("Quote for {} {} -> {}", cmd.amount, cmd.input_token, cmd.output_token);
    println!("  Slippage: {} bps ({}%)", cmd.slippage, cmd.slippage as f64 / 100.0);
    println!("  Direct routes only: {}", cmd.direct_only);
    println!("\n[Not yet implemented]");

    Ok(())
}

/// Handle swap command
async fn swap_command(cmd: SwapCmd) -> Result<()> {
    tracing::info!("Preparing swap: {} -> {}", cmd.input_token, cmd.output_token);

    if cmd.dry_run {
        tracing::info!("DRY RUN mode - no transaction will be sent");
    }

    // TODO: Implement swap execution
    // 1. Load config
    // 2. Initialize clients
    // 3. Fetch quote
    // 4. Confirm with user (if not --yes)
    // 5. Execute swap
    // 6. Confirm transaction

    println!("Swap: {} {} -> {}", cmd.amount, cmd.input_token, cmd.output_token);
    println!("  Slippage: {} bps", cmd.slippage);

    if cmd.dry_run {
        println!("  Mode: DRY RUN (simulation only)");
    } else if !cmd.yes {
        println!("\nConfirmation required: use --yes to skip prompt");
        println!("[Not yet implemented]");
        return Ok(());
    }

    println!("\n[Not yet implemented]");

    Ok(())
}

/// Handle backtest command
async fn backtest_command(cmd: BacktestCmd) -> Result<()> {
    tracing::info!("Starting backtest for {}", cmd.pair);
    tracing::info!("Parameters: {} days, ${} capital", cmd.days, cmd.capital);

    // TODO: Implement backtesting
    // 1. Load config
    // 2. Fetch historical data
    // 3. Initialize strategy with overrides
    // 4. Run simulation
    // 5. Generate report
    // 6. Export if requested

    println!("Backtest Configuration:");
    println!("  Pair: {}", cmd.pair);
    println!("  Period: {} days", cmd.days);
    println!("  Starting Capital: ${:.2}", cmd.capital);

    if let Some(z) = cmd.z_threshold {
        println!("  Z-Threshold: {} (override)", z);
    }

    if let Some(l) = cmd.lookback {
        println!("  Lookback: {} periods (override)", l);
    }

    if let Some(ref csv) = cmd.export_csv {
        println!("  Export CSV: {}", csv.display());
    }

    if let Some(ref json) = cmd.export_json {
        println!("  Export JSON: {}", json.display());
    }

    println!("\n[Not yet implemented]");

    Ok(())
}

/// Handle resume command
async fn resume_command(cmd: ResumeCmd) -> Result<()> {
    use crate::domain::GuardStatus;
    use std::io::{self, Write};

    tracing::info!("Checking BalanceGuard status...");

    // Load current status from file
    let status = match GuardStatus::load(&cmd.data_dir)? {
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
                resume_status.save(&cmd.data_dir)?;
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
    resumed_status.save(&cmd.data_dir)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_app_parse_run() {
        let args = vec!["butters", "run", "--config", "test.toml"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Run(cmd) => {
                assert_eq!(cmd.config, PathBuf::from("test.toml"));
                assert!(!cmd.paper);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_app_parse_run_with_paper() {
        let args = vec!["butters", "run", "--paper"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Run(cmd) => {
                assert!(cmd.paper);
                assert!(!cmd.live);
                assert!(!cmd.i_accept_losses);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_app_parse_run_with_live() {
        let args = vec!["butters", "run", "--live", "--i-accept-losses"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Run(cmd) => {
                assert!(cmd.live);
                assert!(cmd.i_accept_losses);
                assert!(!cmd.paper);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_app_parse_run_live_without_accept() {
        // This parses successfully, but the runtime check should fail
        let args = vec!["butters", "run", "--live"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Run(cmd) => {
                assert!(cmd.live);
                assert!(!cmd.i_accept_losses);
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_app_parse_status() {
        let args = vec!["butters", "status", "--detailed", "--format", "json"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Status(cmd) => {
                assert!(cmd.detailed);
                assert_eq!(cmd.format, "json");
            }
            _ => panic!("Expected Status command"),
        }
    }

    #[test]
    fn test_cli_app_parse_quote() {
        let args = vec!["butters", "quote", "SOL", "USDC", "1.0"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Quote(cmd) => {
                assert_eq!(cmd.input_token, "SOL");
                assert_eq!(cmd.output_token, "USDC");
                assert_eq!(cmd.amount, 1.0);
                assert_eq!(cmd.slippage, 50);
            }
            _ => panic!("Expected Quote command"),
        }
    }

    #[test]
    fn test_cli_app_parse_quote_with_slippage() {
        let args = vec!["butters", "quote", "SOL", "USDC", "1.0", "--slippage", "100"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Quote(cmd) => {
                assert_eq!(cmd.slippage, 100);
                assert!(!cmd.direct_only);
            }
            _ => panic!("Expected Quote command"),
        }
    }

    #[test]
    fn test_cli_app_parse_swap() {
        let args = vec!["butters", "swap", "SOL", "USDC", "1.0", "--slippage", "50"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Swap(cmd) => {
                assert_eq!(cmd.input_token, "SOL");
                assert_eq!(cmd.output_token, "USDC");
                assert_eq!(cmd.amount, 1.0);
                assert_eq!(cmd.slippage, 50);
                assert!(!cmd.yes);
                assert!(!cmd.dry_run);
            }
            _ => panic!("Expected Swap command"),
        }
    }

    #[test]
    fn test_cli_app_parse_swap_with_flags() {
        let args = vec!["butters", "swap", "SOL", "USDC", "1.0", "--yes", "--dry-run"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Swap(cmd) => {
                assert!(cmd.yes);
                assert!(cmd.dry_run);
            }
            _ => panic!("Expected Swap command"),
        }
    }

    #[test]
    fn test_cli_app_parse_backtest() {
        let args = vec!["butters", "backtest", "--pair", "SOL/USDC", "--days", "30"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Backtest(cmd) => {
                assert_eq!(cmd.pair, "SOL/USDC");
                assert_eq!(cmd.days, 30);
                assert_eq!(cmd.capital, 10000.0);
            }
            _ => panic!("Expected Backtest command"),
        }
    }

    #[test]
    fn test_cli_app_parse_backtest_with_overrides() {
        let args = vec![
            "butters", "backtest",
            "--pair", "SOL/USDC",
            "--days", "60",
            "--capital", "50000",
            "--z-threshold", "2.0",
            "--lookback", "30"
        ];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Backtest(cmd) => {
                assert_eq!(cmd.days, 60);
                assert_eq!(cmd.capital, 50000.0);
                assert_eq!(cmd.z_threshold, Some(2.0));
                assert_eq!(cmd.lookback, Some(30));
            }
            _ => panic!("Expected Backtest command"),
        }
    }

    #[test]
    fn test_cli_app_parse_backtest_with_exports() {
        let args = vec![
            "butters", "backtest",
            "--pair", "SOL/USDC",
            "--export-csv", "results.csv",
            "--export-json", "results.json"
        ];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Backtest(cmd) => {
                assert_eq!(cmd.export_csv, Some(PathBuf::from("results.csv")));
                assert_eq!(cmd.export_json, Some(PathBuf::from("results.json")));
            }
            _ => panic!("Expected Backtest command"),
        }
    }

    #[test]
    fn test_global_flags() {
        let args = vec!["butters", "-v", "--debug", "status"];
        let app = CliApp::try_parse_from(args).unwrap();

        assert!(app.verbose);
        assert!(app.debug);
    }

    #[test]
    fn test_default_config_path() {
        let args = vec!["butters", "run"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Run(cmd) => {
                assert_eq!(cmd.config, PathBuf::from("config/mainnet.toml"));
            }
            _ => panic!("Expected Run command"),
        }
    }

    #[test]
    fn test_cli_app_parse_resume() {
        let args = vec!["butters", "resume"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Resume(cmd) => {
                assert_eq!(cmd.config, PathBuf::from("config/mainnet.toml"));
                assert_eq!(cmd.data_dir, PathBuf::from("data"));
                assert!(!cmd.force);
                assert!(!cmd.reset_cumulative);
            }
            _ => panic!("Expected Resume command"),
        }
    }

    #[test]
    fn test_cli_app_parse_resume_with_force() {
        let args = vec!["butters", "resume", "--force"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Resume(cmd) => {
                assert!(cmd.force);
                assert!(!cmd.reset_cumulative);
            }
            _ => panic!("Expected Resume command"),
        }
    }

    #[test]
    fn test_cli_app_parse_resume_with_reset() {
        let args = vec!["butters", "resume", "--force", "--reset-cumulative"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Resume(cmd) => {
                assert!(cmd.force);
                assert!(cmd.reset_cumulative);
            }
            _ => panic!("Expected Resume command"),
        }
    }

    #[test]
    fn test_cli_app_parse_resume_with_data_dir() {
        let args = vec!["butters", "resume", "--data-dir", "/custom/data"];
        let app = CliApp::try_parse_from(args).unwrap();

        match app.command {
            Command::Resume(cmd) => {
                assert_eq!(cmd.data_dir, PathBuf::from("/custom/data"));
            }
            _ => panic!("Expected Resume command"),
        }
    }
}
