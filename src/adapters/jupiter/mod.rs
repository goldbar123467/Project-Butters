//! Jupiter Adapter
//!
//! Implementation of the ExecutionPort for Jupiter DEX aggregator.
//! Handles quote fetching, swap building, and transaction execution.

mod client;
mod quote;
mod swap;

pub use client::JupiterClient;
pub use quote::{QuoteRequest, QuoteResponse};
pub use swap::{SwapRequest, SwapResponse, SwapResult};

#[cfg(test)]
mod contract_tests;
