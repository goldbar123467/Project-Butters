//! Jito Bundle Adapter
//!
//! MEV-protected transaction bundles via Jito Block Engine.
//! Provides frontrunning protection and atomic multi-transaction execution.

mod client;
mod config;
mod error;
mod execution;
mod types;

pub use client::JitoBundleClient;
pub use config::JitoConfig;
pub use error::JitoError;
pub use execution::JitoExecutionAdapter;
pub use types::{
    BundleRequest, BundleResponse, BundleResult, BundleStatus, BundleStatusResponse, TipAccount,
};
