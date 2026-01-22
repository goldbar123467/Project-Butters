//! Jupiter Adapter
//!
//! Implementation of the ExecutionPort for Jupiter DEX aggregator.
//! Handles quote fetching, swap building, and transaction execution.
//! Also provides token list fetching and price APIs.

mod client;
mod quote;
mod swap;
mod token_list;

pub use client::JupiterClient;
pub use quote::{QuoteRequest, QuoteResponse};
pub use swap::SwapRequest;
pub use token_list::{
    JupiterToken, JupiterTokenFetcher,
    TokenCategory, TokenPrice, TrendingInterval,
};

#[cfg(test)]
mod contract_tests;
