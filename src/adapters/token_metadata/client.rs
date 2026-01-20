//! Token Metadata Client
//!
//! Fetches token metadata from Solana RPC using `getAccountInfo` with jsonParsed encoding.
//! Provides information about mint authority, freeze authority, supply, and decimals.

use std::time::Duration;
use reqwest::{Client, StatusCode};
use serde_json::json;
use thiserror::Error;

use super::types::{
    AccountData, AccountInfoResponse, TokenMetadata,
};

/// Errors that can occur when fetching token metadata
#[derive(Debug, Error)]
pub enum TokenMetadataError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("Failed to parse response: {0}")]
    ParseError(String),

    #[error("Account not found: {0}")]
    AccountNotFound(String),

    #[error("Invalid account data: {0}")]
    InvalidAccountData(String),

    #[error("Not a mint account: {0}")]
    NotMintAccount(String),

    #[error("Rate limited, try again later")]
    RateLimited,

    #[error("RPC error: {0}")]
    RpcError(String),
}

/// Configuration for the TokenMetadataClient
#[derive(Debug, Clone)]
pub struct TokenMetadataConfig {
    /// Solana RPC endpoint URL
    pub rpc_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Number of retry attempts
    pub max_retries: u32,
    /// Base delay for exponential backoff (milliseconds)
    pub retry_base_delay_ms: u64,
}

impl Default for TokenMetadataConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://api.mainnet-beta.solana.com".to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 3,
            retry_base_delay_ms: 500,
        }
    }
}

impl TokenMetadataConfig {
    /// Create config with a custom RPC URL
    pub fn with_rpc_url(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            ..Default::default()
        }
    }
}

/// Client for fetching token metadata from Solana RPC
#[derive(Debug, Clone)]
pub struct TokenMetadataClient {
    config: TokenMetadataConfig,
    http: Client,
}

impl TokenMetadataClient {
    /// Create a new TokenMetadataClient with default configuration
    pub fn new() -> Result<Self, TokenMetadataError> {
        Self::with_config(TokenMetadataConfig::default())
    }

    /// Create a new TokenMetadataClient with a custom RPC URL
    pub fn with_rpc_url(rpc_url: impl Into<String>) -> Result<Self, TokenMetadataError> {
        Self::with_config(TokenMetadataConfig::with_rpc_url(rpc_url))
    }

    /// Create a new TokenMetadataClient with custom configuration
    pub fn with_config(config: TokenMetadataConfig) -> Result<Self, TokenMetadataError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()?;

        Ok(Self { config, http })
    }

    /// Fetch complete token metadata for a mint address
    pub async fn get_token_metadata(&self, mint: &str) -> Result<TokenMetadata, TokenMetadataError> {
        let response = self.get_account_info(mint).await?;
        self.parse_mint_account(mint, response)
    }

    /// Check if mint authority is revoked (safe for meme coins)
    pub async fn is_mint_authority_revoked(&self, mint: &str) -> Result<bool, TokenMetadataError> {
        let metadata = self.get_token_metadata(mint).await?;
        Ok(metadata.authority.mint_authority_revoked)
    }

    /// Check if freeze authority is revoked (safe for meme coins)
    pub async fn is_freeze_authority_revoked(&self, mint: &str) -> Result<bool, TokenMetadataError> {
        let metadata = self.get_token_metadata(mint).await?;
        Ok(metadata.authority.freeze_authority_revoked)
    }

    /// Check if token is safe (both authorities revoked)
    pub async fn is_token_safe(&self, mint: &str) -> Result<bool, TokenMetadataError> {
        let metadata = self.get_token_metadata(mint).await?;
        Ok(metadata.is_safe())
    }

    /// Get token supply in base units
    pub async fn get_supply(&self, mint: &str) -> Result<u64, TokenMetadataError> {
        let metadata = self.get_token_metadata(mint).await?;
        Ok(metadata.supply)
    }

    /// Get token decimals
    pub async fn get_decimals(&self, mint: &str) -> Result<u8, TokenMetadataError> {
        let metadata = self.get_token_metadata(mint).await?;
        Ok(metadata.decimals)
    }

    /// Internal: Make getAccountInfo RPC call with retry logic
    async fn get_account_info(&self, mint: &str) -> Result<AccountInfoResponse, TokenMetadataError> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                mint,
                {
                    "encoding": "jsonParsed"
                }
            ]
        });

        self.execute_with_retry(|| async {
            self.http
                .post(&self.config.rpc_url)
                .json(&request_body)
                .send()
                .await
                .map_err(TokenMetadataError::from)
        })
        .await
    }

    /// Execute request with retry logic and exponential backoff
    async fn execute_with_retry<F, Fut>(
        &self,
        request_fn: F,
    ) -> Result<AccountInfoResponse, TokenMetadataError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<reqwest::Response, TokenMetadataError>>,
    {
        let mut last_error = None;

        for attempt in 0..self.config.max_retries {
            match request_fn().await {
                Ok(response) => {
                    let status = response.status();

                    // Handle rate limiting with exponential backoff
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        let backoff = Duration::from_millis(
                            self.config.retry_base_delay_ms * 2u64.pow(attempt + 1),
                        );
                        tracing::warn!(
                            "Rate limited (429), backing off for {:?} (attempt {}/{})",
                            backoff,
                            attempt + 1,
                            self.config.max_retries
                        );
                        last_error = Some(TokenMetadataError::RateLimited);
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // Retry on server errors (5xx)
                    if status.is_server_error() {
                        let backoff = Duration::from_millis(
                            self.config.retry_base_delay_ms * (attempt as u64 + 1),
                        );
                        last_error = Some(TokenMetadataError::RpcError(format!(
                            "Server error: {}",
                            status
                        )));
                        tokio::time::sleep(backoff).await;
                        continue;
                    }

                    // Parse the response
                    let body: AccountInfoResponse = response.json().await.map_err(|e| {
                        TokenMetadataError::ParseError(format!("Failed to parse JSON: {}", e))
                    })?;

                    return Ok(body);
                }
                Err(e) => {
                    last_error = Some(e);
                    let backoff = Duration::from_millis(
                        self.config.retry_base_delay_ms * (attempt as u64 + 1),
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            TokenMetadataError::RpcError("Max retries exceeded".into())
        }))
    }

    /// Parse the account info response into TokenMetadata
    fn parse_mint_account(
        &self,
        mint: &str,
        response: AccountInfoResponse,
    ) -> Result<TokenMetadata, TokenMetadataError> {
        let result = response
            .result
            .ok_or_else(|| TokenMetadataError::RpcError("No result in response".into()))?;

        let value = result
            .value
            .ok_or_else(|| TokenMetadataError::AccountNotFound(mint.to_string()))?;

        // Extract parsed data
        let parsed = match value.data {
            AccountData::Parsed(parsed) => parsed,
            AccountData::Raw(_) => {
                return Err(TokenMetadataError::InvalidAccountData(
                    "Expected jsonParsed encoding, got raw data".into(),
                ))
            }
        };

        // Verify this is a mint account
        if parsed.parsed.account_type != "mint" {
            return Err(TokenMetadataError::NotMintAccount(format!(
                "Account type is '{}', expected 'mint'",
                parsed.parsed.account_type
            )));
        }

        let mint_info = parsed.parsed.info;

        // Parse supply
        let supply: u64 = mint_info.supply.parse().map_err(|e| {
            TokenMetadataError::ParseError(format!("Failed to parse supply: {}", e))
        })?;

        Ok(TokenMetadata::new(
            mint.to_string(),
            supply,
            mint_info.decimals,
            mint_info.is_initialized,
            mint_info.mint_authority,
            mint_info.freeze_authority,
        ))
    }

    /// Get the configured RPC URL
    pub fn rpc_url(&self) -> &str {
        &self.config.rpc_url
    }
}

impl Default for TokenMetadataClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default TokenMetadataClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = TokenMetadataConfig::default();
        assert_eq!(config.rpc_url, "https://api.mainnet-beta.solana.com");
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay_ms, 500);
    }

    #[test]
    fn test_config_with_rpc_url() {
        let config = TokenMetadataConfig::with_rpc_url("https://custom-rpc.example.com");
        assert_eq!(config.rpc_url, "https://custom-rpc.example.com");
    }

    #[test]
    fn test_client_creation() {
        let client = TokenMetadataClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_with_rpc_url() {
        let client = TokenMetadataClient::with_rpc_url("https://devnet.solana.com");
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.rpc_url(), "https://devnet.solana.com");
    }

    #[test]
    fn test_parse_mint_account_success() {
        let client = TokenMetadataClient::new().unwrap();
        let response = AccountInfoResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(super::super::types::AccountInfoResult {
                value: Some(super::super::types::AccountInfoValue {
                    data: AccountData::Parsed(super::super::types::ParsedAccountData {
                        parsed: super::super::types::ParsedInfo {
                            info: super::super::types::MintInfo {
                                mint_authority: None,
                                freeze_authority: None,
                                supply: "1000000000000".to_string(),
                                decimals: 9,
                                is_initialized: true,
                            },
                            account_type: "mint".to_string(),
                        },
                        program: "spl-token".to_string(),
                        space: 82,
                    }),
                    executable: false,
                    lamports: 1461600,
                    owner: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
                    rent_epoch: 0,
                }),
            }),
        };

        let result = client.parse_mint_account("TestMint123", response);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert_eq!(metadata.mint, "TestMint123");
        assert_eq!(metadata.supply, 1000000000000);
        assert_eq!(metadata.decimals, 9);
        assert!(metadata.is_initialized);
        assert!(metadata.is_safe());
    }

    #[test]
    fn test_parse_mint_account_not_found() {
        let client = TokenMetadataClient::new().unwrap();
        let response = AccountInfoResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(super::super::types::AccountInfoResult { value: None }),
        };

        let result = client.parse_mint_account("NonExistentMint", response);
        assert!(matches!(result, Err(TokenMetadataError::AccountNotFound(_))));
    }

    #[test]
    fn test_parse_mint_account_with_authorities() {
        let client = TokenMetadataClient::new().unwrap();
        let response = AccountInfoResponse {
            jsonrpc: "2.0".to_string(),
            id: 1,
            result: Some(super::super::types::AccountInfoResult {
                value: Some(super::super::types::AccountInfoValue {
                    data: AccountData::Parsed(super::super::types::ParsedAccountData {
                        parsed: super::super::types::ParsedInfo {
                            info: super::super::types::MintInfo {
                                mint_authority: Some("MintAuth123".to_string()),
                                freeze_authority: Some("FreezeAuth456".to_string()),
                                supply: "500000000".to_string(),
                                decimals: 6,
                                is_initialized: true,
                            },
                            account_type: "mint".to_string(),
                        },
                        program: "spl-token".to_string(),
                        space: 82,
                    }),
                    executable: false,
                    lamports: 1461600,
                    owner: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA".to_string(),
                    rent_epoch: 0,
                }),
            }),
        };

        let result = client.parse_mint_account("UnsafeMint", response);
        assert!(result.is_ok());

        let metadata = result.unwrap();
        assert!(!metadata.is_safe());
        assert!(metadata.authority.can_mint());
        assert!(metadata.authority.can_freeze());
    }

    #[test]
    fn test_default_client() {
        let client = TokenMetadataClient::default();
        assert_eq!(client.rpc_url(), "https://api.mainnet-beta.solana.com");
    }
}
