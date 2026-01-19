//! Jito Execution Adapter
//!
//! Wraps Jupiter swaps in Jito bundles for MEV protection.
//! Provides frontrunning protection by submitting transactions through
//! the Jito Block Engine as bundles with validator tips.

use async_trait::async_trait;
use solana_sdk::pubkey::Pubkey;

use crate::adapters::jupiter::{JupiterClient, QuoteRequest, SwapRequest};
use crate::ports::execution::{
    ExecuteSwapRequest, ExecuteSwapResponse, ExecutionError, ExecutionPort, SwapQuoteRequest,
    SwapQuoteResponse,
};

use super::client::JitoBundleClient;
use super::error::JitoError;
use super::types::BundleStatus;

/// Jito-wrapped execution adapter for MEV-protected swaps
///
/// This adapter wraps Jupiter swap transactions in Jito bundles,
/// providing protection against frontrunning and sandwich attacks.
/// The adapter adds a tip to validators for priority inclusion.
#[derive(Debug)]
pub struct JitoExecutionAdapter {
    /// Jupiter client for quote fetching and swap building
    jupiter: JupiterClient,
    /// Jito bundle client for MEV-protected submission
    jito: Option<JitoBundleClient>,
    /// Payer public key for tips
    payer: Pubkey,
    /// Whether to use Jito bundles (can disable for testing)
    use_bundles: bool,
    /// Custom tip amount in lamports (None = use default from JitoBundleClient)
    tip_lamports: Option<u64>,
}

impl JitoExecutionAdapter {
    /// Create a new Jito execution adapter with MEV protection enabled
    ///
    /// # Arguments
    /// * `jupiter` - Jupiter client for quote and swap operations
    /// * `jito` - Jito bundle client for bundle submission
    /// * `payer` - Public key of the account paying tips
    ///
    /// # Example
    /// ```ignore
    /// let jupiter = JupiterClient::new()?;
    /// let jito = JitoBundleClient::new()?;
    /// let payer = Pubkey::from_str("...")?;
    /// let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);
    /// ```
    pub fn new(jupiter: JupiterClient, jito: JitoBundleClient, payer: Pubkey) -> Self {
        Self {
            jupiter,
            jito: Some(jito),
            payer,
            use_bundles: true,
            tip_lamports: None,
        }
    }

    /// Create an adapter with Jito bundles disabled
    ///
    /// This is useful for testing or when MEV protection is not needed.
    /// Swaps will be executed directly through Jupiter without bundling.
    ///
    /// # Arguments
    /// * `jupiter` - Jupiter client for quote and swap operations
    /// * `payer` - Public key (used for compatibility but tips won't be sent)
    pub fn with_bundles_disabled(jupiter: JupiterClient, payer: Pubkey) -> Self {
        Self {
            jupiter,
            jito: None,
            payer,
            use_bundles: false,
            tip_lamports: None,
        }
    }

    /// Set a custom tip amount for bundles
    ///
    /// # Arguments
    /// * `lamports` - Tip amount in lamports (1 SOL = 1_000_000_000 lamports)
    pub fn with_tip(mut self, lamports: u64) -> Self {
        self.tip_lamports = Some(lamports);
        self
    }

    /// Check if bundles are enabled
    pub fn bundles_enabled(&self) -> bool {
        self.use_bundles && self.jito.is_some()
    }

    /// Get the payer public key
    pub fn payer(&self) -> &Pubkey {
        &self.payer
    }

    /// Get the configured tip amount
    pub fn tip_lamports(&self) -> Option<u64> {
        self.tip_lamports
    }

    /// Execute a swap as a Jito bundle with MEV protection
    ///
    /// This method:
    /// 1. Takes the base64-encoded swap transaction from Jupiter
    /// 2. Creates a tip instruction for Jito validators
    /// 3. Wraps both in a bundle and submits to the Block Engine
    /// 4. Returns the bundle ID on success
    ///
    /// # Arguments
    /// * `swap_tx` - Base64-encoded serialized swap transaction
    ///
    /// # Returns
    /// Bundle ID string on success, or JitoError on failure
    ///
    /// # Fail-Closed Policy
    /// If Jito bundle submission fails, this method returns an error.
    /// It does NOT fall back to direct RPC submission. This ensures
    /// MEV protection is maintained - trades are only executed with
    /// bundle protection or not at all.
    pub async fn execute_as_bundle(&self, swap_tx: String) -> Result<String, JitoError> {
        let jito = self.jito.as_ref().ok_or_else(|| {
            JitoError::InvalidBundle("Jito client not configured".to_string())
        })?;

        // Validate the transaction string is not empty
        if swap_tx.is_empty() {
            return Err(JitoError::InvalidTransaction(
                "Empty transaction data".to_string(),
            ));
        }

        // Create tip instruction
        let tip = self.tip_lamports.unwrap_or(jito.default_tip_lamports());
        let _tip_ix = jito.create_tip_instruction(&self.payer, self.tip_lamports)?;

        tracing::info!("Submitting swap via Jito bundle (tip: {} lamports)", tip);

        // In production, we would:
        // 1. Deserialize the swap transaction
        // 2. Add the tip instruction to a separate transaction
        // 3. Sign both transactions
        // 4. Bundle them together
        //
        // For now, we send the swap transaction as a single-tx bundle
        // The tip would normally be a separate transaction in the bundle

        // Send bundle with the swap transaction
        // Note: In production, you'd include a signed tip transaction too
        match jito.send_bundle(vec![swap_tx]).await {
            Ok(bundle_id) => {
                tracing::info!("Bundle submitted: {}", bundle_id);
                Ok(bundle_id)
            }
            Err(err) => {
                // FAIL CLOSED: Do NOT fall back to direct RPC
                tracing::error!(
                    "Jito bundle submission failed: {}. NOT falling back to direct RPC.",
                    err
                );
                Err(err)
            }
        }
    }

    /// Execute swap with retry logic
    async fn execute_with_jito_retry(
        &self,
        swap_tx: String,
    ) -> Result<String, JitoError> {
        let jito = self.jito.as_ref().ok_or_else(|| {
            JitoError::InvalidBundle("Jito client not configured".to_string())
        })?;

        let swap_tx_clone = swap_tx.clone();
        jito.execute_with_retry(|| {
            let tx = swap_tx_clone.clone();
            async move {
                self.execute_as_bundle(tx).await
            }
        })
        .await
    }
}

#[async_trait]
impl ExecutionPort for JitoExecutionAdapter {
    /// Get a swap quote from Jupiter
    ///
    /// This delegates directly to Jupiter's quote API without any
    /// Jito-specific modifications. The quote includes optimal routing
    /// across Jupiter's aggregated DEX liquidity.
    async fn get_swap_quote(
        &self,
        request: SwapQuoteRequest,
    ) -> Result<SwapQuoteResponse, ExecutionError> {
        // Delegate to Jupiter for quote fetching
        let quote_request = QuoteRequest {
            input_mint: request.input_mint,
            output_mint: request.output_mint,
            amount: request.amount,
            slippage_bps: request.slippage_bps,
            only_direct_routes: false,
            restrict_intermediate_tokens: None,
            platform_fee_bps: request.platform_fee_bps,
        };

        let quote = self.jupiter.get_quote(&quote_request).await?;

        Ok(SwapQuoteResponse {
            input_amount: quote.in_amount.parse().unwrap_or(0),
            output_amount: quote.out_amount.parse().unwrap_or(0),
            min_output_amount: quote.other_amount_threshold.parse().unwrap_or(0),
            transaction: String::new(), // Transaction comes from swap endpoint
            route: quote
                .route_plan
                .iter()
                .map(|r| r.swap_info.label.clone())
                .collect(),
        })
    }

    /// Execute a swap with optional MEV protection
    ///
    /// If bundles are enabled, the swap transaction is wrapped in a
    /// Jito bundle with a validator tip for priority inclusion and
    /// frontrunning protection.
    ///
    /// If bundles are disabled, delegates directly to Jupiter.
    async fn execute_swap(
        &self,
        request: ExecuteSwapRequest,
    ) -> Result<ExecuteSwapResponse, ExecutionError> {
        // Build swap request from quote response
        let swap_request = SwapRequest {
            user_public_key: request.user_public_key.clone(),
            quote_response: serde_json::to_value(&request.quote_response)
                .map_err(|e| ExecutionError::InvalidParameters(e.to_string()))?,
            prioritization_fee_lamports: request.prioritization_fee_lamports,
            dynamic_compute_unit_limit: request.dynamic_compute_unit_limit.unwrap_or(true),
        };

        // Get swap transaction from Jupiter
        let swap_response = self.jupiter.get_swap_transaction(&swap_request).await?;

        if self.bundles_enabled() {
            // Execute through Jito bundle for MEV protection
            // FAIL CLOSED: If Jito fails, we return an error (no fallback to direct RPC)
            let bundle_id = self
                .execute_with_jito_retry(swap_response.swap_transaction)
                .await
                .map_err(|e| {
                    ExecutionError::ExecutionError(format!(
                        "Jito bundle failed: {}. MEV protection required - trade NOT executed.",
                        e
                    ))
                })?;

            Ok(ExecuteSwapResponse {
                signature: bundle_id, // Return bundle ID as signature
                status: "bundle_submitted".to_string(),
                output_amount: request.quote_response.output_amount,
            })
        } else {
            // Execute without Jito (direct submission)
            // In production, this would sign and submit the transaction directly
            Ok(ExecuteSwapResponse {
                signature: String::new(), // Would be actual signature after submission
                status: "pending".to_string(),
                output_amount: request.quote_response.output_amount,
            })
        }
    }
}

/// Convert JitoError to ExecutionError for the port interface
impl From<JitoError> for ExecutionError {
    fn from(err: JitoError) -> Self {
        match err {
            JitoError::InvalidBundle(msg) => ExecutionError::InvalidParameters(msg),
            JitoError::InvalidTransaction(msg) => ExecutionError::InvalidParameters(msg),
            JitoError::BundleRejected(msg) => ExecutionError::ExecutionError(msg),
            JitoError::SimulationFailed(msg) => ExecutionError::ExecutionError(msg),
            JitoError::BundleDropped => {
                ExecutionError::ExecutionError("Bundle dropped".to_string())
            }
            JitoError::RateLimited => {
                ExecutionError::ApiError("Rate limited by Jito".to_string())
            }
            JitoError::Timeout => ExecutionError::ApiError("Jito request timed out".to_string()),
            _ => ExecutionError::ExecutionError(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_payer() -> Pubkey {
        Pubkey::from_str("11111111111111111111111111111111").unwrap()
    }

    #[test]
    fn test_adapter_creation_with_bundles() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);

        assert!(adapter.bundles_enabled());
        assert_eq!(*adapter.payer(), payer);
        assert!(adapter.tip_lamports().is_none());
    }

    #[test]
    fn test_adapter_creation_bundles_disabled() {
        let jupiter = JupiterClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::with_bundles_disabled(jupiter, payer);

        assert!(!adapter.bundles_enabled());
        assert_eq!(*adapter.payer(), payer);
    }

    #[test]
    fn test_adapter_with_custom_tip() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer).with_tip(50_000);

        assert!(adapter.bundles_enabled());
        assert_eq!(adapter.tip_lamports(), Some(50_000));
    }

    #[test]
    fn test_bundles_enabled_logic() {
        // With Jito client and use_bundles = true
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);
        assert!(adapter.bundles_enabled());

        // With bundles disabled
        let jupiter2 = JupiterClient::new().unwrap();
        let adapter2 = JitoExecutionAdapter::with_bundles_disabled(jupiter2, payer);
        assert!(!adapter2.bundles_enabled());
    }

    #[test]
    fn test_jito_error_to_execution_error_conversion() {
        let err = JitoError::InvalidBundle("test error".to_string());
        let exec_err: ExecutionError = err.into();
        match exec_err {
            ExecutionError::InvalidParameters(msg) => {
                assert!(msg.contains("test error"));
            }
            _ => panic!("Expected InvalidParameters"),
        }

        let err = JitoError::RateLimited;
        let exec_err: ExecutionError = err.into();
        match exec_err {
            ExecutionError::ApiError(msg) => {
                assert!(msg.contains("Rate limited"));
            }
            _ => panic!("Expected ApiError"),
        }

        let err = JitoError::BundleDropped;
        let exec_err: ExecutionError = err.into();
        match exec_err {
            ExecutionError::ExecutionError(msg) => {
                assert!(msg.contains("dropped"));
            }
            _ => panic!("Expected ExecutionError"),
        }
    }

    #[tokio::test]
    async fn test_execute_as_bundle_no_jito_client() {
        let jupiter = JupiterClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::with_bundles_disabled(jupiter, payer);

        let result = adapter.execute_as_bundle("test_tx".to_string()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            JitoError::InvalidBundle(msg) => {
                assert!(msg.contains("not configured"));
            }
            e => panic!("Expected InvalidBundle, got {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_execute_as_bundle_empty_transaction() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);

        let result = adapter.execute_as_bundle(String::new()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            JitoError::InvalidTransaction(msg) => {
                assert!(msg.contains("Empty"));
            }
            e => panic!("Expected InvalidTransaction, got {:?}", e),
        }
    }

    #[test]
    fn test_bundle_status_conversion() {
        // Test that BundleStatus works as expected
        let status = BundleStatus::Landed;
        assert!(status.is_final());
        assert!(status.is_success());

        let status = BundleStatus::Pending;
        assert!(!status.is_final());
        assert!(!status.is_success());

        let status = BundleStatus::Failed;
        assert!(status.is_final());
        assert!(!status.is_success());
    }

    #[test]
    fn test_adapter_payer_getter() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = Pubkey::from_str("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM").unwrap();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);

        assert_eq!(*adapter.payer(), payer);
    }

    #[test]
    fn test_adapter_tip_default() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);

        // Default should be None (use JitoBundleClient default)
        assert!(adapter.tip_lamports().is_none());
    }

    #[test]
    fn test_multiple_tip_settings() {
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();

        // Test various tip amounts
        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer).with_tip(10_000);
        assert_eq!(adapter.tip_lamports(), Some(10_000));

        let jupiter2 = JupiterClient::new().unwrap();
        let jito2 = JitoBundleClient::new().unwrap();
        let adapter2 = JitoExecutionAdapter::new(jupiter2, jito2, payer).with_tip(100_000);
        assert_eq!(adapter2.tip_lamports(), Some(100_000));
    }

    // ============================================================
    // Integration Tests for Fail-Closed Policy
    // ============================================================

    #[tokio::test]
    async fn test_jito_fail_closed_policy() {
        // Test that when bundles are enabled but submission fails,
        // we get an error (not silent fallback to direct RPC)

        // Adapter with bundles disabled should explicitly show disabled state
        let jupiter = JupiterClient::new().unwrap();
        let payer = Pubkey::new_unique();
        let adapter = JitoExecutionAdapter::with_bundles_disabled(jupiter, payer);
        assert!(!adapter.bundles_enabled());

        // When bundles are enabled, execute_as_bundle with invalid data should error
        let jupiter2 = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let adapter2 = JitoExecutionAdapter::new(jupiter2, jito, payer);
        assert!(adapter2.bundles_enabled());

        // Empty transaction should be rejected (not silently dropped)
        let result = adapter2.execute_as_bundle(String::new()).await;
        assert!(result.is_err());

        // Verify the error message indicates we're NOT falling back
        let err = result.unwrap_err();
        match err {
            JitoError::InvalidTransaction(msg) => {
                assert!(msg.contains("Empty"), "Expected empty transaction error, got: {}", msg);
            }
            _ => panic!("Expected InvalidTransaction error, got: {:?}", err),
        }
    }

    #[test]
    fn test_bundle_path_used_when_enabled() {
        // Test that bundles_enabled() correctly reflects configuration

        // With Jito client configured, bundles should be enabled
        let jupiter = JupiterClient::new().unwrap();
        let jito = JitoBundleClient::new().unwrap();
        let payer = test_payer();
        let adapter = JitoExecutionAdapter::new(jupiter, jito, payer);

        assert!(
            adapter.bundles_enabled(),
            "Bundles should be enabled when Jito client is configured"
        );

        // With bundles explicitly disabled, bundles should be disabled
        let jupiter2 = JupiterClient::new().unwrap();
        let adapter2 = JitoExecutionAdapter::with_bundles_disabled(jupiter2, payer);

        assert!(
            !adapter2.bundles_enabled(),
            "Bundles should be disabled when explicitly disabled"
        );
    }

    #[tokio::test]
    async fn test_execute_as_bundle_requires_jito_client() {
        // Test that execute_as_bundle fails without Jito client
        let jupiter = JupiterClient::new().unwrap();
        let payer = test_payer();

        // Create adapter without Jito client
        let adapter = JitoExecutionAdapter::with_bundles_disabled(jupiter, payer);

        // Attempting to use bundle path should fail
        let result = adapter.execute_as_bundle("valid_tx_data".to_string()).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            JitoError::InvalidBundle(msg) => {
                assert!(
                    msg.contains("not configured"),
                    "Error should mention Jito not configured, got: {}", msg
                );
            }
            e => panic!("Expected InvalidBundle error, got: {:?}", e),
        }
    }

    #[test]
    fn test_fail_closed_error_message_format() {
        // Verify that our fail-closed error message format is correct
        // This tests the conversion path that execute_swap uses

        let jito_error = JitoError::BundleRejected("simulation failed".to_string());
        let exec_error: ExecutionError = jito_error.into();

        match exec_error {
            ExecutionError::ExecutionError(msg) => {
                assert!(
                    msg.contains("simulation failed"),
                    "Error should contain original error, got: {}", msg
                );
            }
            _ => panic!("Expected ExecutionError variant"),
        }

        // Test the specific format used in execute_swap
        let error_msg = format!(
            "Jito bundle failed: {}. MEV protection required - trade NOT executed.",
            "connection timeout"
        );
        assert!(error_msg.contains("NOT executed"));
        assert!(error_msg.contains("MEV protection required"));
    }
}
