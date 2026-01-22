//! Honeypot Detection Adapter
//!
//! Production implementation of honeypot detection for Solana tokens.
//! Detects various honeypot mechanisms including:
//! - Token-2022 extensions (TransferHook, PermanentDelegate, NonTransferable)
//! - Freeze and Mint authorities
//! - Custom token programs (non-SPL)
//! - Failed sell simulations via Jupiter

mod cache;
mod detector;
mod known_hooks;
mod sell_simulator;
mod token2022;

pub use cache::HoneypotCache;
pub use detector::SolanaHoneypotDetector;
pub use known_hooks::known_safe_hook_programs;
pub use sell_simulator::SellSimulator;
pub use token2022::{
    parse_authorities, parse_token2022_extensions, validate_token_program, AuthorityInfo,
    ExtensionType, TokenProgram,
};
