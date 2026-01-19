//! Jupiter Swap Types
//!
//! Request and response structures for Jupiter V6 swap API.
//! Handles transaction building and execution.

use serde::{Deserialize, Serialize};

/// Request parameters for building a swap transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapRequest {
    /// User's public key (wallet address)
    pub user_public_key: String,
    /// The full quote response from /quote endpoint
    pub quote_response: serde_json::Value,
    /// Optional prioritization fee in lamports for faster inclusion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prioritization_fee_lamports: Option<u64>,
    /// Whether to use dynamic compute unit limit calculation
    #[serde(default = "default_dynamic_compute_unit_limit")]
    pub dynamic_compute_unit_limit: bool,
}

fn default_dynamic_compute_unit_limit() -> bool {
    true
}

impl SwapRequest {
    /// Create a new swap request with required parameters
    pub fn new(
        user_public_key: String,
        quote_response: serde_json::Value,
    ) -> Self {
        Self {
            user_public_key,
            quote_response,
            prioritization_fee_lamports: None,
            dynamic_compute_unit_limit: true,
        }
    }

    /// Set prioritization fee for faster transaction inclusion
    pub fn with_priority_fee(mut self, lamports: u64) -> Self {
        self.prioritization_fee_lamports = Some(lamports);
        self
    }

    /// Set dynamic compute unit limit flag
    pub fn with_dynamic_compute_limit(mut self, enabled: bool) -> Self {
        self.dynamic_compute_unit_limit = enabled;
        self
    }
}

/// Response from Jupiter swap API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResponse {
    /// Base64 encoded serialized transaction ready to sign and send
    pub swap_transaction: String,
    /// Last valid block height for this transaction
    pub last_valid_block_height: u64,
    /// Prioritization fee applied (in lamports)
    #[serde(default)]
    pub prioritization_fee_lamports: u64,
}

impl SwapResponse {
    /// Get the transaction bytes from base64
    pub fn transaction_bytes(&self) -> Result<Vec<u8>, base64::DecodeError> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.decode(&self.swap_transaction)
    }

    /// Check if transaction is still valid based on current block height
    pub fn is_valid_at_height(&self, current_height: u64) -> bool {
        current_height <= self.last_valid_block_height
    }

    /// Get blocks remaining until expiry
    pub fn blocks_remaining(&self, current_height: u64) -> i64 {
        self.last_valid_block_height as i64 - current_height as i64
    }
}

/// Result of a swap execution attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResult {
    /// Transaction signature (hash)
    pub signature: String,
    /// Execution status
    pub status: SwapStatus,
    /// Actual output amount received (in base units)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_amount: Option<u64>,
    /// Block height where transaction was confirmed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_at_height: Option<u64>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Status of a swap execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwapStatus {
    /// Transaction submitted to network
    Pending,
    /// Transaction confirmed on-chain
    Confirmed,
    /// Transaction failed
    Failed,
    /// Transaction expired (past last_valid_block_height)
    Expired,
}

impl SwapResult {
    /// Create a new pending swap result
    pub fn pending(signature: String) -> Self {
        Self {
            signature,
            status: SwapStatus::Pending,
            output_amount: None,
            confirmed_at_height: None,
            error: None,
        }
    }

    /// Mark swap as confirmed with output amount
    pub fn confirmed(
        mut self,
        output_amount: u64,
        confirmed_at_height: u64,
    ) -> Self {
        self.status = SwapStatus::Confirmed;
        self.output_amount = Some(output_amount);
        self.confirmed_at_height = Some(confirmed_at_height);
        self
    }

    /// Mark swap as failed with error message
    pub fn failed(mut self, error: String) -> Self {
        self.status = SwapStatus::Failed;
        self.error = Some(error);
        self
    }

    /// Mark swap as expired
    pub fn expired(mut self) -> Self {
        self.status = SwapStatus::Expired;
        self.error = Some("Transaction expired".to_string());
        self
    }

    /// Check if swap is complete (confirmed or failed)
    pub fn is_complete(&self) -> bool {
        matches!(
            self.status,
            SwapStatus::Confirmed | SwapStatus::Failed | SwapStatus::Expired
        )
    }

    /// Check if swap succeeded
    pub fn is_success(&self) -> bool {
        self.status == SwapStatus::Confirmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_request_new() {
        let quote_json = serde_json::json!({
            "inputMint": "SOL",
            "outputMint": "USDC",
            "inAmount": "1000000000",
            "outAmount": "150000000"
        });

        let req = SwapRequest::new(
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".to_string(),
            quote_json,
        );

        assert!(req.prioritization_fee_lamports.is_none());
        assert!(req.dynamic_compute_unit_limit);
    }

    #[test]
    fn test_swap_request_builder() {
        let quote_json = serde_json::json!({});

        let req = SwapRequest::new(
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM".to_string(),
            quote_json,
        )
        .with_priority_fee(10000)
        .with_dynamic_compute_limit(false);

        assert_eq!(req.prioritization_fee_lamports, Some(10000));
        assert!(!req.dynamic_compute_unit_limit);
    }

    #[test]
    fn test_swap_response_parsing() {
        let json = r#"{
            "swapTransaction": "AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "lastValidBlockHeight": 123456789,
            "prioritizationFeeLamports": 5000
        }"#;

        let response: SwapResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.last_valid_block_height, 123456789);
        assert_eq!(response.prioritization_fee_lamports, 5000);
        assert!(response.transaction_bytes().is_ok());
    }

    #[test]
    fn test_swap_response_validity() {
        let response = SwapResponse {
            swap_transaction: "test".to_string(),
            last_valid_block_height: 1000,
            prioritization_fee_lamports: 0,
        };

        assert!(response.is_valid_at_height(999));
        assert!(response.is_valid_at_height(1000));
        assert!(!response.is_valid_at_height(1001));
    }

    #[test]
    fn test_swap_response_blocks_remaining() {
        let response = SwapResponse {
            swap_transaction: "test".to_string(),
            last_valid_block_height: 1000,
            prioritization_fee_lamports: 0,
        };

        assert_eq!(response.blocks_remaining(990), 10);
        assert_eq!(response.blocks_remaining(1000), 0);
        assert_eq!(response.blocks_remaining(1010), -10);
    }

    #[test]
    fn test_swap_result_pending() {
        let result = SwapResult::pending("sig123".to_string());

        assert_eq!(result.signature, "sig123");
        assert_eq!(result.status, SwapStatus::Pending);
        assert!(result.output_amount.is_none());
        assert!(!result.is_complete());
        assert!(!result.is_success());
    }

    #[test]
    fn test_swap_result_confirmed() {
        let result = SwapResult::pending("sig123".to_string())
            .confirmed(150000000, 123456);

        assert_eq!(result.status, SwapStatus::Confirmed);
        assert_eq!(result.output_amount, Some(150000000));
        assert_eq!(result.confirmed_at_height, Some(123456));
        assert!(result.is_complete());
        assert!(result.is_success());
    }

    #[test]
    fn test_swap_result_failed() {
        let result = SwapResult::pending("sig123".to_string())
            .failed("Slippage exceeded".to_string());

        assert_eq!(result.status, SwapStatus::Failed);
        assert_eq!(result.error, Some("Slippage exceeded".to_string()));
        assert!(result.is_complete());
        assert!(!result.is_success());
    }

    #[test]
    fn test_swap_result_expired() {
        let result = SwapResult::pending("sig123".to_string())
            .expired();

        assert_eq!(result.status, SwapStatus::Expired);
        assert!(result.error.is_some());
        assert!(result.is_complete());
        assert!(!result.is_success());
    }

    #[test]
    fn test_swap_status_serialization() {
        let status = SwapStatus::Confirmed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""confirmed""#);
    }

    #[test]
    fn test_swap_request_serialization() {
        let quote = serde_json::json!({"test": "data"});
        let req = SwapRequest::new("wallet123".to_string(), quote)
            .with_priority_fee(5000);

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["userPublicKey"], "wallet123");
        assert_eq!(json["prioritizationFeeLamports"], 5000);
        assert_eq!(json["dynamicComputeUnitLimit"], true);
    }
}
