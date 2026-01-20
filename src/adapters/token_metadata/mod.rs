//! Token Metadata Adapter
//!
//! Fetches token metadata from Solana RPC, including:
//! - Mint authority status (revoked = safe)
//! - Freeze authority status (revoked = safe)
//! - Token supply and decimals
//!
//! Uses Solana's `getAccountInfo` RPC method with `jsonParsed` encoding.
//!
//! # Example
//!
//! ```rust,ignore
//! use butters::adapters::token_metadata::{TokenMetadataClient, TokenMetadata};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = TokenMetadataClient::new()?;
//!
//!     // Fetch full metadata for a token
//!     let metadata = client.get_token_metadata("So11111111111111111111111111111111111111112").await?;
//!     println!("Supply: {}", metadata.supply_adjusted());
//!     println!("Safe: {}", metadata.is_safe());
//!
//!     // Quick safety check
//!     let is_safe = client.is_token_safe("SomeMintAddress").await?;
//!     Ok(())
//! }
//! ```

mod client;
mod types;

pub use client::TokenMetadataClient;
pub use types::{AuthorityInfo, TokenMetadata};
