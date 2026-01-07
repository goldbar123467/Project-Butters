//! Jito Bundle Client
//!
//! HTTP client for Jito Block Engine API.
//! Handles bundle submission, status checking, and MEV-protected transactions.

use std::time::{Duration, Instant};

use reqwest::Client;
use solana_sdk::{
    instruction::Instruction,
    pubkey::Pubkey,
    system_instruction,
};

use super::config::{JitoConfig, tip_accounts};
use super::error::JitoError;
use super::types::{
    BundleRequest, BundleResult, BundleStatus,
    GetBundleStatusesRequest, GetBundleStatusesResponse, JsonRpcResponse,
};

/// Jito Block Engine client for bundle submission
#[derive(Debug, Clone)]
pub struct JitoBundleClient {
    /// Client configuration
    config: JitoConfig,
    /// HTTP client
    http: Client,
}

impl JitoBundleClient {
    /// Create a new Jito client with default configuration
    pub fn new() -> Result<Self, JitoError> {
        Self::with_config(JitoConfig::default())
    }

    /// Create a new Jito client with custom configuration
    pub fn with_config(config: JitoConfig) -> Result<Self, JitoError> {
        let http = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| JitoError::HttpError(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, http })
    }

    /// Send a bundle of transactions to the block engine
    ///
    /// # Arguments
    /// * `transactions` - Base64-encoded serialized transactions
    ///
    /// # Returns
    /// Bundle ID on success
    pub async fn send_bundle(&self, transactions: Vec<String>) -> Result<String, JitoError> {
        // Validate bundle
        if transactions.is_empty() {
            return Err(JitoError::InvalidBundle("Bundle cannot be empty".into()));
        }

        if transactions.len() > 5 {
            return Err(JitoError::InvalidBundle(
                "Bundle cannot contain more than 5 transactions".into(),
            ));
        }

        let url = format!("{}/api/v1/bundles", self.config.block_engine_url);
        let request = BundleRequest::new(transactions);

        let mut req_builder = self.http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request);

        // Add API token if configured
        if let Some(ref token) = self.config.api_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = req_builder
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 429 {
            return Err(JitoError::RateLimited);
        }

        let response_text = response.text().await?;

        // Parse JSON-RPC response
        let rpc_response: JsonRpcResponse<String> = serde_json::from_str(&response_text)?;

        // Check for error
        if let Some(error) = rpc_response.error {
            return Err(JitoError::ApiError {
                code: error.code,
                message: error.message,
            });
        }

        // Extract bundle ID
        rpc_response.result.ok_or_else(|| {
            JitoError::ApiError {
                code: -1,
                message: "No bundle ID in response".into(),
            }
        })
    }

    /// Get the status of a bundle by ID
    ///
    /// # Arguments
    /// * `bundle_id` - The bundle UUID returned from send_bundle
    ///
    /// # Returns
    /// Bundle status information
    pub async fn get_bundle_status(&self, bundle_id: &str) -> Result<BundleStatus, JitoError> {
        let url = format!("{}/api/v1/bundles", self.config.block_engine_url);
        let request = GetBundleStatusesRequest::new(vec![bundle_id.to_string()]);

        let mut req_builder = self.http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request);

        if let Some(ref token) = self.config.api_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
        }

        let response = req_builder.send().await?;

        if response.status().as_u16() == 429 {
            return Err(JitoError::RateLimited);
        }

        let response_text = response.text().await?;

        // Parse JSON-RPC response
        let rpc_response: JsonRpcResponse<GetBundleStatusesResponse> =
            serde_json::from_str(&response_text)?;

        if let Some(error) = rpc_response.error {
            return Err(JitoError::StatusCheckFailed(error.message));
        }

        let statuses = rpc_response.result.ok_or_else(|| {
            JitoError::StatusCheckFailed("No status in response".into())
        })?;

        // Find our bundle status
        let entry = statuses.value.into_iter()
            .find(|e| e.bundle_id == bundle_id)
            .ok_or_else(|| JitoError::StatusCheckFailed("Bundle not found".into()))?;

        Ok(Self::parse_status(&entry.status))
    }

    /// Send a bundle and wait for it to land or fail
    ///
    /// # Arguments
    /// * `transactions` - Base64-encoded serialized transactions
    /// * `timeout` - Maximum time to wait for bundle to land
    /// * `poll_interval` - How often to check status
    ///
    /// # Returns
    /// Bundle result with final status
    pub async fn send_bundle_and_wait(
        &self,
        transactions: Vec<String>,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<BundleResult, JitoError> {
        let start = Instant::now();

        // Send the bundle
        let bundle_id = self.send_bundle(transactions.clone()).await?;

        // Poll for status
        loop {
            if start.elapsed() > timeout {
                return Err(JitoError::Timeout);
            }

            let status = self.get_bundle_status(&bundle_id).await?;

            if status.is_final() {
                let time_to_land_ms = start.elapsed().as_millis() as u64;

                return Ok(BundleResult {
                    bundle_id: bundle_id.clone(),
                    status,
                    slot: None, // Would need additional API call for slot
                    signatures: Vec::new(), // Signatures from original transactions
                    time_to_land_ms: Some(time_to_land_ms),
                });
            }

            tokio::time::sleep(poll_interval).await;
        }
    }

    /// Execute an operation with retry logic and exponential backoff
    ///
    /// # Arguments
    /// * `operation` - Async closure that performs the operation
    ///
    /// # Returns
    /// Result of the operation or the last error after all retries
    pub async fn execute_with_retry<F, Fut, T>(&self, operation: F) -> Result<T, JitoError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, JitoError>>,
    {
        let mut last_error = JitoError::NetworkError("No attempts made".into());
        let mut delay_ms = self.config.retry_delay_ms;

        for attempt in 0..self.config.max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = e.clone();

                    // Only retry on retryable errors
                    if !e.is_retryable() {
                        return Err(e);
                    }

                    // Don't sleep after the last attempt
                    if attempt < self.config.max_retries - 1 {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        delay_ms *= 2; // Exponential backoff
                    }
                }
            }
        }

        Err(JitoError::MaxRetriesExceeded {
            attempts: self.config.max_retries,
            last_error: last_error.to_string(),
        })
    }

    /// Create a tip instruction to pay validators
    ///
    /// # Arguments
    /// * `payer` - The public key of the account paying the tip
    /// * `tip_lamports` - Amount of tip in lamports (uses default if None)
    ///
    /// # Returns
    /// Instruction to transfer tip to a random Jito tip account
    pub fn create_tip_instruction(
        &self,
        payer: &Pubkey,
        tip_lamports: Option<u64>,
    ) -> Result<Instruction, JitoError> {
        let tip_amount = tip_lamports.unwrap_or(self.config.default_tip_lamports);

        if tip_amount == 0 {
            return Err(JitoError::InvalidBundle("Tip amount cannot be zero".into()));
        }

        let tip_account_str = tip_accounts::random_tip_account();
        let tip_account = tip_account_str.parse::<Pubkey>()
            .map_err(|e| JitoError::InvalidTransaction(format!("Invalid tip account: {}", e)))?;

        Ok(system_instruction::transfer(payer, &tip_account, tip_amount))
    }

    /// Create a tip instruction to a specific tip account
    ///
    /// # Arguments
    /// * `payer` - The public key of the account paying the tip
    /// * `tip_account` - The tip account to send to
    /// * `tip_lamports` - Amount of tip in lamports
    ///
    /// # Returns
    /// Instruction to transfer tip
    pub fn create_tip_instruction_to(
        &self,
        payer: &Pubkey,
        tip_account: &Pubkey,
        tip_lamports: u64,
    ) -> Instruction {
        system_instruction::transfer(payer, tip_account, tip_lamports)
    }

    /// Get the configured block engine URL
    pub fn block_engine_url(&self) -> &str {
        &self.config.block_engine_url
    }

    /// Get the default tip amount in lamports
    pub fn default_tip_lamports(&self) -> u64 {
        self.config.default_tip_lamports
    }

    /// Parse status string from API response
    fn parse_status(status_str: &str) -> BundleStatus {
        match status_str.to_lowercase().as_str() {
            "pending" => BundleStatus::Pending,
            "processing" => BundleStatus::Processing,
            "landed" => BundleStatus::Landed,
            "failed" => BundleStatus::Failed,
            "dropped" => BundleStatus::Dropped,
            _ => BundleStatus::Unknown,
        }
    }
}

impl Default for JitoBundleClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default JitoBundleClient")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_client_creation_default() {
        let client = JitoBundleClient::new();
        assert!(client.is_ok());
        let client = client.unwrap();
        assert!(client.block_engine_url().contains("mainnet.block-engine.jito.wtf"));
    }

    #[test]
    fn test_client_creation_with_config() {
        let config = JitoConfig::default()
            .with_tip(50_000)
            .with_timeout(Duration::from_secs(60));

        let client = JitoBundleClient::with_config(config);
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.default_tip_lamports(), 50_000);
    }

    #[test]
    fn test_client_with_api_token() {
        let config = JitoConfig::default()
            .with_api_token("test-token-123".to_string());

        let client = JitoBundleClient::with_config(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_tip_instruction() {
        let client = JitoBundleClient::new().unwrap();
        let payer = Pubkey::from_str("11111111111111111111111111111111").unwrap();

        let instruction = client.create_tip_instruction(&payer, Some(10_000));
        assert!(instruction.is_ok());

        let ix = instruction.unwrap();
        assert_eq!(ix.program_id, solana_sdk::system_program::ID);
    }

    #[test]
    fn test_create_tip_instruction_default_amount() {
        let client = JitoBundleClient::new().unwrap();
        let payer = Pubkey::from_str("11111111111111111111111111111111").unwrap();

        let instruction = client.create_tip_instruction(&payer, None);
        assert!(instruction.is_ok());
    }

    #[test]
    fn test_create_tip_instruction_zero_amount() {
        let client = JitoBundleClient::new().unwrap();
        let payer = Pubkey::from_str("11111111111111111111111111111111").unwrap();

        let instruction = client.create_tip_instruction(&payer, Some(0));
        assert!(instruction.is_err());

        let err = instruction.unwrap_err();
        assert!(matches!(err, JitoError::InvalidBundle(_)));
    }

    #[test]
    fn test_create_tip_instruction_to_specific_account() {
        let client = JitoBundleClient::new().unwrap();
        let payer = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        let tip_account = Pubkey::from_str("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5").unwrap();

        let instruction = client.create_tip_instruction_to(&payer, &tip_account, 25_000);
        assert_eq!(instruction.program_id, solana_sdk::system_program::ID);
    }

    #[test]
    fn test_parse_status() {
        assert_eq!(JitoBundleClient::parse_status("pending"), BundleStatus::Pending);
        assert_eq!(JitoBundleClient::parse_status("LANDED"), BundleStatus::Landed);
        assert_eq!(JitoBundleClient::parse_status("Failed"), BundleStatus::Failed);
        assert_eq!(JitoBundleClient::parse_status("dropped"), BundleStatus::Dropped);
        assert_eq!(JitoBundleClient::parse_status("unknown_status"), BundleStatus::Unknown);
    }

    #[tokio::test]
    async fn test_send_bundle_empty_validation() {
        let client = JitoBundleClient::new().unwrap();
        let result = client.send_bundle(vec![]).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            JitoError::InvalidBundle(msg) => {
                assert!(msg.contains("empty"));
            }
            _ => panic!("Expected InvalidBundle error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_send_bundle_too_many_transactions() {
        let client = JitoBundleClient::new().unwrap();
        let transactions = vec![
            "tx1".to_string(),
            "tx2".to_string(),
            "tx3".to_string(),
            "tx4".to_string(),
            "tx5".to_string(),
            "tx6".to_string(), // 6th transaction - exceeds limit
        ];

        let result = client.send_bundle(transactions).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            JitoError::InvalidBundle(msg) => {
                assert!(msg.contains("more than 5"));
            }
            _ => panic!("Expected InvalidBundle error, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_execute_with_retry_success_first_try() {
        let client = JitoBundleClient::new().unwrap();

        let result = client.execute_with_retry(|| async {
            Ok::<_, JitoError>("success".to_string())
        }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_execute_with_retry_non_retryable_error() {
        let client = JitoBundleClient::new().unwrap();

        let result = client.execute_with_retry(|| async {
            Err::<String, _>(JitoError::InvalidBundle("test".into()))
        }).await;

        assert!(result.is_err());
        // Should fail immediately without retrying since InvalidBundle is not retryable
        match result.unwrap_err() {
            JitoError::InvalidBundle(_) => {}
            e => panic!("Expected InvalidBundle, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_execute_with_retry_max_retries() {
        let config = JitoConfig {
            max_retries: 2,
            retry_delay_ms: 10, // Short delay for test
            ..Default::default()
        };
        let client = JitoBundleClient::with_config(config).unwrap();

        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result = client.execute_with_retry(|| {
            let counter = counter_clone.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<String, _>(JitoError::Timeout) // Retryable error
            }
        }).await;

        assert!(result.is_err());
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2); // 2 retries

        match result.unwrap_err() {
            JitoError::MaxRetriesExceeded { attempts, .. } => {
                assert_eq!(attempts, 2);
            }
            e => panic!("Expected MaxRetriesExceeded, got {:?}", e),
        }
    }

    #[test]
    fn test_bundle_request_format() {
        let txs = vec!["tx1_base64".to_string(), "tx2_base64".to_string()];
        let request = BundleRequest::new(txs.clone());

        assert_eq!(request.jsonrpc, "2.0");
        assert_eq!(request.method, "sendBundle");
        assert_eq!(request.params.len(), 1);
        assert_eq!(request.params[0], txs);
    }

    #[test]
    fn test_default_client() {
        let client = JitoBundleClient::default();
        assert!(client.block_engine_url().contains("jito.wtf"));
        assert_eq!(client.default_tip_lamports(), 10_000);
    }
}
