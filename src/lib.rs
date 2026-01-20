#![allow(dead_code, unused_imports, unused_variables)]
//! Butters - Jupiter Mean Reversion DEX Trading Bot Library
//!
//! A conservative mean reversion trading strategy for Solana via Jupiter aggregator.
//!
//! # Modules
//!
//! - `domain`: Core business logic (Position, Trade, Portfolio, BalanceGuard)
//! - `ports`: Trait abstractions (MarketDataPort, ExecutionPort, StrategyPort)
//! - `strategy`: Signal generation (MeanReversion, ZScoreGate, Regime Detection)
//! - `adapters`: External implementations (Jupiter, Solana, CLI)
//! - `config`: Configuration loading and validation
//! - `application`: Orchestrator and use cases
//! - `meme`: Multi-token meme coin trading module

pub mod domain;
pub mod ports;
pub mod strategy;
pub mod adapters;
pub mod config;
pub mod application;
pub mod meme;
