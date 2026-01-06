use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("Failed to load keypair from file: {0}")]
    LoadError(String),
    #[error("Failed to sign transaction: {0}")]
    SigningError(String),
    #[error("Invalid keypair bytes: {0}")]
    InvalidKeypair(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Wallet manager for loading and signing with Solana keypairs
pub struct WalletManager {
    keypair: Keypair,
}

impl WalletManager {
    /// Load keypair from a file path (JSON array format)
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, WalletError> {
        let contents = fs::read_to_string(path.as_ref())
            .map_err(|e| WalletError::LoadError(format!("Failed to read file: {}", e)))?;

        // Parse JSON array of bytes
        let bytes: Vec<u8> = serde_json::from_str(&contents)
            .map_err(|e| WalletError::LoadError(format!("Invalid JSON format: {}", e)))?;

        Self::from_bytes(&bytes)
    }

    /// Load keypair from raw bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, WalletError> {
        let keypair = Keypair::try_from(bytes)
            .map_err(|e| WalletError::InvalidKeypair(e.to_string()))?;

        Ok(Self { keypair })
    }

    /// Create a new random keypair (for testing)
    pub fn new_random() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    /// Get the public key as a string
    pub fn public_key(&self) -> String {
        self.keypair.pubkey().to_string()
    }

    /// Get the public key as Pubkey
    pub fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }

    /// Sign a transaction
    pub fn sign_transaction(&self, transaction: &mut Transaction) -> Result<(), WalletError> {
        transaction
            .try_sign(&[&self.keypair], transaction.message.recent_blockhash)
            .map_err(|e| WalletError::SigningError(e.to_string()))
    }

    /// Sign a message and return the signature
    pub fn sign_message(&self, message: &[u8]) -> Signature {
        self.keypair.sign_message(message)
    }

    /// Get keypair reference (for advanced use cases)
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Export keypair as bytes (use with caution)
    pub fn to_bytes(&self) -> Vec<u8> {
        self.keypair.to_bytes().to_vec()
    }

    /// Save keypair to file (JSON array format)
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), WalletError> {
        let bytes = self.to_bytes();
        let json = serde_json::to_string(&bytes)
            .map_err(|e| WalletError::LoadError(format!("Failed to serialize: {}", e)))?;

        fs::write(path.as_ref(), json)?;
        Ok(())
    }
}

// Implement Clone for WalletManager by re-creating from bytes
impl Clone for WalletManager {
    fn clone(&self) -> Self {
        Self {
            keypair: Keypair::try_from(&self.keypair.to_bytes()[..]).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_new_random_wallet() {
        let wallet = WalletManager::new_random();
        let pubkey = wallet.public_key();
        assert!(!pubkey.is_empty());
        assert_eq!(pubkey.len(), 44); // Base58 encoded pubkey length
    }

    #[test]
    fn test_from_bytes() {
        let wallet1 = WalletManager::new_random();
        let bytes = wallet1.to_bytes();

        let wallet2 = WalletManager::from_bytes(&bytes).unwrap();
        assert_eq!(wallet1.public_key(), wallet2.public_key());
    }

    #[test]
    fn test_sign_message() {
        let wallet = WalletManager::new_random();
        let message = b"Hello, Solana!";
        let signature = wallet.sign_message(message);

        // Verify signature length (64 bytes)
        assert_eq!(signature.as_ref().len(), 64);
    }

    #[test]
    fn test_save_and_load() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let wallet1 = WalletManager::new_random();

        // Manually write JSON to temp file
        let bytes = wallet1.to_bytes();
        let json = serde_json::to_string(&bytes).unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        // Load from file
        let wallet2 = WalletManager::from_file(temp_file.path()).unwrap();
        assert_eq!(wallet1.public_key(), wallet2.public_key());
    }

    #[test]
    fn test_clone_wallet() {
        let wallet1 = WalletManager::new_random();
        let wallet2 = wallet1.clone();
        assert_eq!(wallet1.public_key(), wallet2.public_key());
    }

    #[test]
    fn test_invalid_bytes() {
        let invalid_bytes = vec![0u8; 10]; // Too short
        let result = WalletManager::from_bytes(&invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_json_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"not valid json").unwrap();
        temp_file.flush().unwrap();

        let result = WalletManager::from_file(temp_file.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_pubkey_formats() {
        let wallet = WalletManager::new_random();
        let pubkey_string = wallet.public_key();
        let pubkey_struct = wallet.pubkey();

        assert_eq!(pubkey_string, pubkey_struct.to_string());
    }
}
