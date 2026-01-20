//! Domain Layer - Core business logic for Butters trading bot
//!
//! This module contains pure domain types and logic with no external dependencies.
//! All external interactions happen through the ports layer.

pub mod position;
pub mod trade;
pub mod portfolio;
pub mod risk;
pub mod signal;
pub mod known_programs;
pub mod tx_validator;
pub mod balance_guard;
pub mod meme_balance_guard;
pub mod liquidity_guard;
pub mod honeypot_detector;
pub mod rug_detector;

pub use tx_validator::TransactionValidator;
pub use balance_guard::{BalanceGuard, ExpectedDelta, GuardStatus};

// Meme coin trading safety modules
