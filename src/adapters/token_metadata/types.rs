//! Token Metadata Types
//!
//! Core types for token metadata and authority information fetched from Solana RPC.

use serde::{Deserialize, Serialize};

/// Represents the authority information for a token mint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthorityInfo {
    /// Mint authority - can create new tokens (None = revoked, which is safe)
    pub mint_authority: Option<String>,
    /// Freeze authority - can freeze token accounts (None = revoked, which is safe)
    pub freeze_authority: Option<String>,
    /// Whether mint authority has been revoked (safe for meme coins)
    pub mint_authority_revoked: bool,
    /// Whether freeze authority has been revoked (safe for meme coins)
    pub freeze_authority_revoked: bool,
}

impl AuthorityInfo {
    /// Create a new AuthorityInfo from raw authority values
    pub fn new(mint_authority: Option<String>, freeze_authority: Option<String>) -> Self {
        Self {
            mint_authority_revoked: mint_authority.is_none(),
            freeze_authority_revoked: freeze_authority.is_none(),
            mint_authority,
            freeze_authority,
        }
    }

    /// Check if the token is safe (both authorities revoked)
    pub fn is_safe(&self) -> bool {
        self.mint_authority_revoked && self.freeze_authority_revoked
    }

    /// Returns true if mint authority can still create tokens
    pub fn can_mint(&self) -> bool {
        !self.mint_authority_revoked
    }

    /// Returns true if freeze authority can freeze accounts
    pub fn can_freeze(&self) -> bool {
        !self.freeze_authority_revoked
    }
}

impl Default for AuthorityInfo {
    fn default() -> Self {
        Self {
            mint_authority: None,
            freeze_authority: None,
            mint_authority_revoked: true,
            freeze_authority_revoked: true,
        }
    }
}

/// Full token metadata including supply, decimals, and authorities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMetadata {
    /// Token mint address
    pub mint: String,
    /// Token supply in base units
    pub supply: u64,
    /// Number of decimal places
    pub decimals: u8,
    /// Whether the mint has been initialized
    pub is_initialized: bool,
    /// Authority information
    pub authority: AuthorityInfo,
}

impl TokenMetadata {
    /// Create new token metadata
    pub fn new(
        mint: String,
        supply: u64,
        decimals: u8,
        is_initialized: bool,
        mint_authority: Option<String>,
        freeze_authority: Option<String>,
    ) -> Self {
        Self {
            mint,
            supply,
            decimals,
            is_initialized,
            authority: AuthorityInfo::new(mint_authority, freeze_authority),
        }
    }

    /// Get the supply in human-readable format (adjusted for decimals)
    pub fn supply_adjusted(&self) -> f64 {
        self.supply as f64 / 10f64.powi(self.decimals as i32)
    }

    /// Check if the token is safe (both authorities revoked)
    pub fn is_safe(&self) -> bool {
        self.authority.is_safe()
    }
}

/// Solana RPC response for getAccountInfo with jsonParsed encoding
#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfoResponse {
    pub jsonrpc: String,
    pub id: u64,
    pub result: Option<AccountInfoResult>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfoResult {
    pub value: Option<AccountInfoValue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInfoValue {
    pub data: AccountData,
    pub executable: bool,
    pub lamports: u64,
    pub owner: String,
    #[serde(rename = "rentEpoch")]
    pub rent_epoch: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AccountData {
    Parsed(ParsedAccountData),
    Raw(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParsedAccountData {
    pub parsed: ParsedInfo,
    pub program: String,
    pub space: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ParsedInfo {
    pub info: MintInfo,
    #[serde(rename = "type")]
    pub account_type: String,
}

/// Mint account information from SPL Token program
#[derive(Debug, Clone, Deserialize)]
pub struct MintInfo {
    #[serde(rename = "mintAuthority")]
    pub mint_authority: Option<String>,
    #[serde(rename = "freezeAuthority")]
    pub freeze_authority: Option<String>,
    pub supply: String,
    pub decimals: u8,
    #[serde(rename = "isInitialized")]
    pub is_initialized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authority_info_new_revoked() {
        let auth = AuthorityInfo::new(None, None);
        assert!(auth.mint_authority_revoked);
        assert!(auth.freeze_authority_revoked);
        assert!(auth.is_safe());
        assert!(!auth.can_mint());
        assert!(!auth.can_freeze());
    }

    #[test]
    fn test_authority_info_new_active() {
        let auth = AuthorityInfo::new(
            Some("MintAuthority123".to_string()),
            Some("FreezeAuthority456".to_string()),
        );
        assert!(!auth.mint_authority_revoked);
        assert!(!auth.freeze_authority_revoked);
        assert!(!auth.is_safe());
        assert!(auth.can_mint());
        assert!(auth.can_freeze());
    }

    #[test]
    fn test_authority_info_mixed() {
        let auth = AuthorityInfo::new(None, Some("FreezeAuthority456".to_string()));
        assert!(auth.mint_authority_revoked);
        assert!(!auth.freeze_authority_revoked);
        assert!(!auth.is_safe()); // Not safe because freeze is not revoked
    }

    #[test]
    fn test_token_metadata_new() {
        let meta = TokenMetadata::new(
            "SoMint123456".to_string(),
            1_000_000_000_000_000, // 1 billion with 6 decimals
            6,
            true,
            None,
            None,
        );
        assert_eq!(meta.mint, "SoMint123456");
        assert_eq!(meta.decimals, 6);
        assert!(meta.is_initialized);
        assert!(meta.is_safe());
    }

    #[test]
    fn test_token_metadata_supply_adjusted() {
        let meta = TokenMetadata::new(
            "TestMint".to_string(),
            1_000_000_000, // 1 billion base units
            6,
            true,
            None,
            None,
        );
        let adjusted = meta.supply_adjusted();
        assert!((adjusted - 1000.0).abs() < 0.001); // 1000 tokens
    }

    #[test]
    fn test_token_metadata_supply_adjusted_9_decimals() {
        let meta = TokenMetadata::new(
            "TestMint".to_string(),
            1_000_000_000_000_000_000, // 1 billion tokens with 9 decimals
            9,
            true,
            None,
            None,
        );
        let adjusted = meta.supply_adjusted();
        assert!((adjusted - 1_000_000_000.0).abs() < 0.001); // 1 billion tokens
    }

    #[test]
    fn test_authority_info_default() {
        let auth = AuthorityInfo::default();
        assert!(auth.is_safe());
        assert!(auth.mint_authority.is_none());
        assert!(auth.freeze_authority.is_none());
    }

    #[test]
    fn test_token_metadata_unsafe() {
        let meta = TokenMetadata::new(
            "UnsafeMint".to_string(),
            1_000_000_000,
            9,
            true,
            Some("MintAuth".to_string()),
            Some("FreezeAuth".to_string()),
        );
        assert!(!meta.is_safe());
    }
}
