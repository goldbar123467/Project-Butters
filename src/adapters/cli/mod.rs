//! CLI Adapter
//!
//! Command-line interface for the Butters trading bot.
//! Uses clap derive macros for argument parsing.

mod commands;

pub use commands::{CliApp, Command, RunCmd, StatusCmd, QuoteCmd, SwapCmd, BacktestCmd, ResumeCmd};

use anyhow::Result;

/// Initialize the CLI application
pub fn init() -> CliApp {
    use clap::Parser;
    CliApp::parse()
}

/// Execute the CLI command
pub async fn execute(app: CliApp) -> Result<()> {
    commands::execute(app).await
}
