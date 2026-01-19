//! Jito Bundle Types
//!
//! Request and response types for Jito Block Engine API.

use serde::{Deserialize, Serialize};

/// Bundle submission request (JSON-RPC format)
#[derive(Debug, Clone, Serialize)]
pub struct BundleRequest {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Request ID
    pub id: u64,
    /// Method name
    pub method: String,
    /// Bundle parameters
    pub params: Vec<Vec<String>>,
}

impl BundleRequest {
    /// Create a new bundle request
    pub fn new(transactions: Vec<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "sendBundle".to_string(),
            params: vec![transactions],
        }
    }
}

/// JSON-RPC response wrapper
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse<T> {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Response ID
    pub id: u64,
    /// Result (if success)
    pub result: Option<T>,
    /// Error (if failure)
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data
    pub data: Option<serde_json::Value>,
}

/// Bundle submission response
#[derive(Debug, Clone, Deserialize)]
pub struct BundleResponse {
    /// Bundle UUID assigned by block engine
    pub bundle_id: String,
}

/// Bundle status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    /// Bundle is pending processing
    Pending,
    /// Bundle is being processed
    Processing,
    /// Bundle landed on chain
    Landed,
    /// Bundle failed to land
    Failed,
    /// Bundle was dropped
    Dropped,
    /// Bundle status unknown
    Unknown,
}

impl BundleStatus {
    /// Check if status is final
    pub fn is_final(&self) -> bool {
        matches!(self, BundleStatus::Landed | BundleStatus::Failed | BundleStatus::Dropped)
    }

    /// Check if status is successful
    pub fn is_success(&self) -> bool {
        matches!(self, BundleStatus::Landed)
    }
}

impl Default for BundleStatus {
    fn default() -> Self {
        BundleStatus::Unknown
    }
}

/// Bundle status response from getBundleStatuses
#[derive(Debug, Clone, Deserialize)]
pub struct BundleStatusResponse {
    /// Bundle ID
    pub bundle_id: String,
    /// Bundle status
    pub status: BundleStatus,
    /// Slot where bundle landed (if landed)
    pub slot: Option<u64>,
    /// Transaction signatures in bundle
    pub transactions: Option<Vec<TransactionStatus>>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Individual transaction status within a bundle
#[derive(Debug, Clone, Deserialize)]
pub struct TransactionStatus {
    /// Transaction signature
    pub signature: String,
    /// Confirmation status
    pub confirmation_status: Option<String>,
    /// Error if transaction failed
    pub err: Option<serde_json::Value>,
}

/// Tip account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TipAccount {
    /// Tip account public key
    pub address: String,
    /// Whether this account is currently active
    pub is_active: bool,
}

/// Result of a bundle submission
#[derive(Debug, Clone)]
pub struct BundleResult {
    /// Bundle UUID
    pub bundle_id: String,
    /// Final status
    pub status: BundleStatus,
    /// Slot where landed (if successful)
    pub slot: Option<u64>,
    /// Transaction signatures
    pub signatures: Vec<String>,
    /// Time to land in milliseconds (if tracked)
    pub time_to_land_ms: Option<u64>,
}

impl BundleResult {
    /// Create a new bundle result from response
    pub fn from_status(bundle_id: String, status_response: BundleStatusResponse) -> Self {
        Self {
            bundle_id,
            status: status_response.status,
            slot: status_response.slot,
            signatures: status_response
                .transactions
                .map(|txs| txs.into_iter().map(|t| t.signature).collect())
                .unwrap_or_default(),
            time_to_land_ms: None,
        }
    }

    /// Check if bundle was successful
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }
}

/// Request to get bundle statuses
#[derive(Debug, Clone, Serialize)]
pub struct GetBundleStatusesRequest {
    /// JSON-RPC version
    pub jsonrpc: String,
    /// Request ID
    pub id: u64,
    /// Method name
    pub method: String,
    /// Bundle IDs to check
    pub params: Vec<Vec<String>>,
}

impl GetBundleStatusesRequest {
    /// Create a new status check request
    pub fn new(bundle_ids: Vec<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "getBundleStatuses".to_string(),
            params: vec![bundle_ids],
        }
    }
}

/// Response from getBundleStatuses
#[derive(Debug, Clone, Deserialize)]
pub struct GetBundleStatusesResponse {
    /// Status results keyed by bundle ID
    pub value: Vec<BundleStatusEntry>,
}

/// Single bundle status entry
#[derive(Debug, Clone, Deserialize)]
pub struct BundleStatusEntry {
    /// Bundle ID
    pub bundle_id: String,
    /// Status
    pub status: String,
    /// Landed slot
    pub landed_slot: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_request_creation() {
        let txs = vec!["tx1".to_string(), "tx2".to_string()];
        let req = BundleRequest::new(txs.clone());

        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.method, "sendBundle");
        assert_eq!(req.params.len(), 1);
        assert_eq!(req.params[0], txs);
    }

    #[test]
    fn test_bundle_status_is_final() {
        assert!(BundleStatus::Landed.is_final());
        assert!(BundleStatus::Failed.is_final());
        assert!(BundleStatus::Dropped.is_final());

        assert!(!BundleStatus::Pending.is_final());
        assert!(!BundleStatus::Processing.is_final());
    }

    #[test]
    fn test_bundle_status_is_success() {
        assert!(BundleStatus::Landed.is_success());

        assert!(!BundleStatus::Failed.is_success());
        assert!(!BundleStatus::Dropped.is_success());
        assert!(!BundleStatus::Pending.is_success());
    }

    #[test]
    fn test_bundle_result_success() {
        let result = BundleResult {
            bundle_id: "test-id".to_string(),
            status: BundleStatus::Landed,
            slot: Some(12345),
            signatures: vec!["sig1".to_string()],
            time_to_land_ms: Some(500),
        };

        assert!(result.is_success());
        assert_eq!(result.slot, Some(12345));
    }

    #[test]
    fn test_get_bundle_statuses_request() {
        let ids = vec!["id1".to_string(), "id2".to_string()];
        let req = GetBundleStatusesRequest::new(ids.clone());

        assert_eq!(req.method, "getBundleStatuses");
        assert_eq!(req.params[0], ids);
    }

    #[test]
    fn test_bundle_status_serialization() {
        let status = BundleStatus::Landed;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"landed\"");

        let parsed: BundleStatus = serde_json::from_str("\"pending\"").unwrap();
        assert_eq!(parsed, BundleStatus::Pending);
    }
}
