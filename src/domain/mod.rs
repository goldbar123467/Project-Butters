//! Domain Layer - Core business logic for Butters trading bot
//!
//! This module contains pure domain types and logic with no external dependencies.
//! All external interactions happen through the ports layer.

pub mod position;
pub mod trade;
pub mod portfolio;
pub mod risk;
pub mod signal;

pub use position::{Position, Side, Status, PositionError};
pub use trade::{Trade, TradeResult, Fee};
pub use portfolio::{Portfolio, Holding};
pub use risk::{RiskLimits, RiskCheck, RiskViolation, InstrumentRiskLimits};
pub use signal::{Signal, SignalType};
