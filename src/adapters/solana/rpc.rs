use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    signature::Signature,
    transaction::Transaction,
};
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SolanaClientError {
    #[error("RPC request failed: {0}")]
    RpcError(String),
    #[error("Transaction failed: {0}")]
    TransactionError(String),
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    #[error("Timeout waiting for confirmation")]
    ConfirmationTimeout,
}

/// Wrapper around Solana RPC client with async-compatible methods
#[derive(Clone)]
pub struct SolanaClient {
    client: Arc<RpcClient>,
}

impl SolanaClient {
    /// Create a new Solana RPC client
    pub fn new(rpc_url: String) -> Self {
        let client = Arc::new(RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed()));
        Self { client }
    }

    /// Get SOL balance for a public key
    pub async fn get_balance(&self, pubkey: &str) -> Result<u64, SolanaClientError> {
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(pubkey)
            .map_err(|e| SolanaClientError::InvalidPublicKey(e.to_string()))?;

        // Spawn blocking to make sync RPC call async-compatible
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            client
                .get_balance(&pubkey)
                .map_err(|e| SolanaClientError::RpcError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Get SPL token account balance
    pub async fn get_token_account_balance(
        &self,
        token_account_pubkey: &str,
    ) -> Result<u64, SolanaClientError> {
        let pubkey = solana_sdk::pubkey::Pubkey::from_str(token_account_pubkey)
            .map_err(|e| SolanaClientError::InvalidPublicKey(e.to_string()))?;

        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            client
                .get_token_account_balance(&pubkey)
                .map_err(|e| SolanaClientError::RpcError(e.to_string()))
                .and_then(|balance| {
                    balance
                        .amount
                        .parse::<u64>()
                        .map_err(|e| SolanaClientError::RpcError(format!("Parse error: {}", e)))
                })
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Send a transaction to the network
    pub async fn send_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, SolanaClientError> {
        let tx = transaction.clone();
        let client = Arc::clone(&self.client);

        tokio::task::spawn_blocking(move || {
            client
                .send_transaction(&tx)
                .map(|sig| sig.to_string())
                .map_err(|e| SolanaClientError::TransactionError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Confirm a transaction with signature
    pub async fn confirm_transaction(
        &self,
        signature_str: &str,
    ) -> Result<bool, SolanaClientError> {
        let signature = Signature::from_str(signature_str)
            .map_err(|e| SolanaClientError::InvalidSignature(e.to_string()))?;

        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            // Poll for confirmation (with timeout handled by RPC client)
            client
                .confirm_transaction(&signature)
                .map_err(|e| SolanaClientError::TransactionError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Send and confirm a transaction in one call
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<String, SolanaClientError> {
        let tx = transaction.clone();
        let client = Arc::clone(&self.client);

        tokio::task::spawn_blocking(move || {
            client
                .send_and_confirm_transaction(&tx)
                .map(|sig| sig.to_string())
                .map_err(|e| SolanaClientError::TransactionError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Get recent blockhash (needed for transaction building)
    pub async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, SolanaClientError> {
        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            client
                .get_latest_blockhash()
                .map_err(|e| SolanaClientError::RpcError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }

    /// Get transaction details by signature
    pub async fn get_transaction(
        &self,
        signature_str: &str,
    ) -> Result<String, SolanaClientError> {
        let signature = Signature::from_str(signature_str)
            .map_err(|e| SolanaClientError::InvalidSignature(e.to_string()))?;

        let client = Arc::clone(&self.client);
        tokio::task::spawn_blocking(move || {
            client
                .get_transaction(&signature, UiTransactionEncoding::Json)
                .map(|tx| serde_json::to_string(&tx).unwrap_or_default())
                .map_err(|e| SolanaClientError::RpcError(e.to_string()))
        })
        .await
        .map_err(|e| SolanaClientError::RpcError(format!("Task join error: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let client = SolanaClient::new("https://api.devnet.solana.com".to_string());
        // Just verify it compiles and constructs
        assert!(std::mem::size_of_val(&client) > 0);
    }

    #[test]
    fn test_error_display() {
        let err = SolanaClientError::RpcError("test".to_string());
        assert!(err.to_string().contains("RPC request failed"));

        let err = SolanaClientError::ConfirmationTimeout;
        assert!(err.to_string().contains("Timeout"));
    }
}
