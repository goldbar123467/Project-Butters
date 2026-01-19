//! Domain Layer - Core business logic for Butters trading bot
//!
//! This module contains pure domain types and logic with no external dependencies.
//! All external interactions happen through the ports layer.
//!
//! ## Safety Modules for Meme Coin Trading
//!
//! The following safety modules are implemented:
//! - `balance_guard`: Pre/post trade balance validation
//! - `tx_validator`: Transaction security validation
//! - `known_programs`: Whitelisted Solana programs
//! - `rug_detector`: Detect potential rug pull patterns
//! - `liquidity_guard`: Monitor and validate liquidity levels
//! - `circuit_breaker`: Emergency halt on anomalies
//! - `quote_validator`: Validate Jupiter quotes for manipulation
//! - `position_persistence`: Crash recovery for open positions

pub mod position;
pub mod trade;
pub mod portfolio;
pub mod risk;
pub mod signal;
pub mod known_programs;
pub mod tx_validator;
pub mod balance_guard;
pub mod rug_detector;
pub mod liquidity_guard;
pub mod circuit_breaker;
pub mod quote_validator;
pub mod position_persistence;

pub use position::{Position, Side, Status, PositionError};
pub use trade::{Trade, TradeResult, Fee};
pub use portfolio::{Portfolio, Holding};
pub use risk::{RiskLimits, RiskCheck, RiskViolation, InstrumentRiskLimits};
pub use signal::{Signal, SignalType};
pub use tx_validator::{TransactionValidator, TxValidationError, TxValidationResult};
pub use balance_guard::{BalanceGuard, BalanceGuardConfig, BalanceGuardError, ExpectedDelta, BalanceSnapshot, GuardStatus};
pub use known_programs::{is_jito_tip_account, jito_tip_pubkeys, JITO_TIP_ACCOUNTS};
pub use rug_detector::{RugPullDetector, RugPullRisk, TokenInfo, RugAnalysisResult, RiskFactor, RugDetectorError};
pub use liquidity_guard::{LiquidityGuard, LiquidityTrend, LiquidityStatus, LiquiditySnapshot, LiquidityGuardError};
pub use circuit_breaker::{CircuitBreaker, CircuitBreakerStatus, CircuitBreakerState, CircuitBreakerError, TradeRecord};
pub use quote_validator::{QuoteValidator, QuoteInfo, QuoteValidationResult, QuoteError};
pub use position_persistence::{PersistedPosition, PositionManager, PositionMetadata, RecoveryStatus, PersistError};
