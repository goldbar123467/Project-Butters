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

pub use position::{Position, Side, Status, PositionError};
pub use trade::{Trade, TradeResult, Fee};
pub use portfolio::{Portfolio, Holding};
pub use risk::{RiskLimits, RiskCheck, RiskViolation, InstrumentRiskLimits};
pub use signal::{Signal, SignalType};
pub use tx_validator::{TransactionValidator, TxValidationError, TxValidationResult};
pub use balance_guard::{BalanceGuard, BalanceGuardConfig, BalanceGuardError, ExpectedDelta, BalanceSnapshot};
pub use known_programs::{is_jito_tip_account, jito_tip_pubkeys, JITO_TIP_ACCOUNTS};
