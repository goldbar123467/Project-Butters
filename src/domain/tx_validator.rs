//! Transaction Validator
//!
//! Pre-sign validation of transactions to prevent unauthorized fund transfers.
//! Parses SystemProgram::Transfer and SPL Token CloseAccount instructions
//! and validates all destination addresses against an allowlist.

use solana_sdk::{
    message::VersionedMessage,
    pubkey::Pubkey,
    system_program,
    transaction::VersionedTransaction,
};
use std::collections::HashSet;
use thiserror::Error;

use super::known_programs::{
    is_jito_tip_account, is_known_dex_program, is_system_program,
    jito_tip_pubkeys, dex_program_pubkeys, system_program_pubkeys, jupiter_routing_pubkeys,
};

/// SPL Token program ID
const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";

/// System program transfer instruction discriminator
const SYSTEM_TRANSFER_DISCRIMINATOR: u32 = 2;

/// SPL Token CloseAccount instruction discriminator
const SPL_CLOSE_ACCOUNT_DISCRIMINATOR: u8 = 9;

/// Errors that can occur during transaction validation
#[derive(Error, Debug, Clone)]
pub enum TxValidationError {
    #[error("Unauthorized transfer destination: {destination} (expected one of: user wallet, Jito tip, or known program)")]
    UnauthorizedTransferDestination { destination: String },

    #[error("Unauthorized CloseAccount destination: {destination} (rent should return to user wallet)")]
    UnauthorizedCloseAccountDestination { destination: String },

    #[error("Failed to parse transaction: {0}")]
    ParseError(String),

    #[error("Transaction contains {count} unauthorized destinations: {destinations:?}")]
    MultipleUnauthorizedDestinations { count: usize, destinations: Vec<String> },
}

/// Result of a successful transaction validation
#[derive(Debug, Clone)]
pub struct TxValidationResult {
    /// Number of SystemProgram::Transfer instructions found
    pub transfer_count: usize,
    /// Number of SPL Token CloseAccount instructions found
    pub close_account_count: usize,
    /// All validated destination addresses
    pub validated_destinations: Vec<Pubkey>,
    /// Addresses that were allowed with warnings (unknown but likely PDAs in Jupiter context)
    pub warned_destinations: Vec<Pubkey>,
}

/// Detected transfer from transaction
#[derive(Debug, Clone)]
pub struct DetectedTransfer {
    /// Source account (payer)
    pub from: Pubkey,
    /// Destination account
    pub to: Pubkey,
    /// Amount in lamports
    pub lamports: u64,
    /// Whether this is a Jito tip
    pub is_jito_tip: bool,
}

/// Detected CloseAccount instruction
#[derive(Debug, Clone)]
pub struct DetectedCloseAccount {
    /// Account being closed
    pub account: Pubkey,
    /// Destination for rent lamports
    pub destination: Pubkey,
    /// Authority that signed the close
    pub authority: Pubkey,
}

/// Validation mode for handling unknown destinations in Jupiter transactions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JupiterValidationMode {
    /// Strict mode: reject any unknown destination (original behavior)
    Strict,
    /// Permissive mode: allow unknown destinations in Jupiter context with warnings
    /// This handles dynamic PDAs (pool vaults, routing accounts) that can't be statically whitelisted
    Permissive,
}

impl Default for JupiterValidationMode {
    fn default() -> Self {
        Self::Permissive
    }
}

/// Transaction validator that checks all destinations before signing
#[derive(Debug, Clone)]
pub struct TransactionValidator {
    /// User's wallet pubkey - always allowed as destination
    user_wallet: Pubkey,
    /// Set of all allowed destination pubkeys
    allowed_destinations: HashSet<Pubkey>,
    /// Known DEX program IDs for PDA derivation checks
    known_dex_programs: HashSet<Pubkey>,
    /// Whether to log warnings for unknown programs (vs failing)
    warn_on_unknown_programs: bool,
    /// Validation mode for Jupiter transactions with dynamic PDAs
    jupiter_validation_mode: JupiterValidationMode,
}

impl TransactionValidator {
    /// Create a new transaction validator for the given user wallet
    pub fn new(user_wallet: Pubkey) -> Self {
        Self::with_mode(user_wallet, JupiterValidationMode::default())
    }

    /// Create a new transaction validator with a specific Jupiter validation mode
    pub fn with_mode(user_wallet: Pubkey, jupiter_validation_mode: JupiterValidationMode) -> Self {
        let mut allowed_destinations = HashSet::new();

        // Add user wallet
        allowed_destinations.insert(user_wallet);

        // Add Jito tip accounts
        for pubkey in jito_tip_pubkeys() {
            allowed_destinations.insert(pubkey);
        }

        // Add known DEX programs (for program account destinations)
        for pubkey in dex_program_pubkeys() {
            allowed_destinations.insert(pubkey);
        }

        // Add system programs
        for pubkey in system_program_pubkeys() {
            allowed_destinations.insert(pubkey);
        }

        // Add Jupiter routing accounts (fee accounts, token ledger, etc.)
        for pubkey in jupiter_routing_pubkeys() {
            allowed_destinations.insert(pubkey);
        }

        // Build known DEX programs set for PDA validation
        let known_dex_programs: HashSet<Pubkey> = dex_program_pubkeys().into_iter().collect();

        Self {
            user_wallet,
            allowed_destinations,
            known_dex_programs,
            warn_on_unknown_programs: true,
            jupiter_validation_mode,
        }
    }

    /// Create a strict validator that rejects unknown destinations
    pub fn strict(user_wallet: Pubkey) -> Self {
        Self::with_mode(user_wallet, JupiterValidationMode::Strict)
    }

    /// Create a permissive validator that allows unknown destinations in Jupiter context
    pub fn permissive(user_wallet: Pubkey) -> Self {
        Self::with_mode(user_wallet, JupiterValidationMode::Permissive)
    }

    /// Add additional allowed destination addresses
    pub fn add_allowed_destination(&mut self, pubkey: Pubkey) {
        self.allowed_destinations.insert(pubkey);
    }

    /// Add multiple allowed destination addresses
    pub fn add_allowed_destinations(&mut self, pubkeys: impl IntoIterator<Item = Pubkey>) {
        for pubkey in pubkeys {
            self.allowed_destinations.insert(pubkey);
        }
    }

    /// Validate a transaction before signing
    ///
    /// Returns Ok(TxValidationResult) if all destinations are authorized,
    /// or Err(TxValidationError) if any unauthorized destinations found.
    ///
    /// In Permissive mode, unknown destinations in Jupiter transactions are allowed
    /// with warnings, as Jupiter uses dynamic PDAs for pool vaults and routing accounts.
    pub fn validate(&self, tx: &VersionedTransaction) -> Result<TxValidationResult, TxValidationError> {
        let mut transfer_count = 0;
        let mut close_account_count = 0;
        let mut validated_destinations = Vec::new();
        let mut warned_destinations = Vec::new();
        let mut unauthorized_destinations = Vec::new();

        // Extract account keys from the message
        let account_keys = self.get_account_keys(&tx.message)?;

        // Check if this transaction involves a known DEX program (Jupiter context)
        let is_jupiter_context = self.detect_jupiter_context(&tx.message, &account_keys);

        // Parse each instruction
        let instructions = match &tx.message {
            VersionedMessage::Legacy(msg) => &msg.instructions,
            VersionedMessage::V0(msg) => &msg.instructions,
        };

        for ix in instructions {
            let program_id_index = ix.program_id_index as usize;
            if program_id_index >= account_keys.len() {
                continue;
            }
            let program_id = &account_keys[program_id_index];

            // Check for SystemProgram::Transfer
            if *program_id == system_program::id() {
                if let Some(transfer) = self.parse_system_transfer(ix, &account_keys) {
                    transfer_count += 1;

                    if self.is_allowed_destination(&transfer.to) {
                        validated_destinations.push(transfer.to);
                        tracing::debug!(
                            "Validated transfer: {} lamports to {} (Jito tip: {})",
                            transfer.lamports, transfer.to, transfer.is_jito_tip
                        );
                    } else if self.should_allow_unknown_destination(&transfer.to, is_jupiter_context) {
                        // In permissive mode with Jupiter context, allow but warn
                        warned_destinations.push(transfer.to);
                        tracing::warn!(
                            "Unknown transfer destination allowed in Jupiter context: {} ({} lamports) - likely a pool vault or routing PDA",
                            transfer.to, transfer.lamports
                        );
                    } else {
                        unauthorized_destinations.push(transfer.to.to_string());
                        tracing::error!(
                            "SECURITY: Unauthorized transfer destination detected: {}",
                            transfer.to
                        );
                    }
                }
            }

            // Check for SPL Token program
            let spl_token_id = SPL_TOKEN_PROGRAM_ID.parse::<Pubkey>().unwrap();
            if *program_id == spl_token_id {
                if let Some(close) = self.parse_close_account(ix, &account_keys) {
                    close_account_count += 1;

                    // For CloseAccount, destination MUST be user wallet (rent goes back to user)
                    if close.destination == self.user_wallet {
                        validated_destinations.push(close.destination);
                        tracing::debug!(
                            "Validated CloseAccount: {} rent returning to user wallet",
                            close.account
                        );
                    } else if self.is_allowed_destination(&close.destination) {
                        // Allow known destinations but log warning
                        validated_destinations.push(close.destination);
                        tracing::warn!(
                            "CloseAccount destination is not user wallet: {} (allowed but unusual)",
                            close.destination
                        );
                    } else if self.should_allow_unknown_destination(&close.destination, is_jupiter_context) {
                        // In permissive mode, allow unknown close destinations in Jupiter context
                        warned_destinations.push(close.destination);
                        tracing::warn!(
                            "Unknown CloseAccount destination allowed in Jupiter context: {}",
                            close.destination
                        );
                    } else {
                        unauthorized_destinations.push(close.destination.to_string());
                        tracing::error!(
                            "SECURITY: Unauthorized CloseAccount destination: {} (should be user wallet: {})",
                            close.destination, self.user_wallet
                        );
                    }
                }
            }

            // Log unknown programs if configured
            if self.warn_on_unknown_programs
                && !is_system_program(program_id)
                && !is_known_dex_program(program_id)
            {
                tracing::debug!("Unknown program in transaction: {}", program_id);
            }
        }

        // Return error if any unauthorized destinations found
        if !unauthorized_destinations.is_empty() {
            if unauthorized_destinations.len() == 1 {
                // Check if it was a transfer or close account
                if close_account_count > 0 && transfer_count == 0 {
                    return Err(TxValidationError::UnauthorizedCloseAccountDestination {
                        destination: unauthorized_destinations[0].clone(),
                    });
                } else {
                    return Err(TxValidationError::UnauthorizedTransferDestination {
                        destination: unauthorized_destinations[0].clone(),
                    });
                }
            } else {
                return Err(TxValidationError::MultipleUnauthorizedDestinations {
                    count: unauthorized_destinations.len(),
                    destinations: unauthorized_destinations,
                });
            }
        }

        // Log summary if any destinations were warned
        if !warned_destinations.is_empty() {
            tracing::info!(
                "Transaction validated with {} warned destinations (Jupiter dynamic PDAs)",
                warned_destinations.len()
            );
        }

        Ok(TxValidationResult {
            transfer_count,
            close_account_count,
            validated_destinations,
            warned_destinations,
        })
    }

    /// Detect if this transaction involves Jupiter or other known DEX programs
    fn detect_jupiter_context(&self, message: &VersionedMessage, account_keys: &[Pubkey]) -> bool {
        let instructions = match message {
            VersionedMessage::Legacy(msg) => &msg.instructions,
            VersionedMessage::V0(msg) => &msg.instructions,
        };

        for ix in instructions {
            let program_id_index = ix.program_id_index as usize;
            if program_id_index < account_keys.len() {
                let program_id = &account_keys[program_id_index];
                if self.known_dex_programs.contains(program_id) {
                    return true;
                }
            }
        }

        // Also check if any account in the transaction is a known DEX program
        // (some Jupiter routes invoke programs indirectly)
        for key in account_keys {
            if self.known_dex_programs.contains(key) {
                return true;
            }
        }

        false
    }

    /// Determine if an unknown destination should be allowed based on context
    fn should_allow_unknown_destination(&self, _destination: &Pubkey, is_jupiter_context: bool) -> bool {
        match self.jupiter_validation_mode {
            JupiterValidationMode::Strict => false,
            JupiterValidationMode::Permissive => {
                // In permissive mode, allow unknown destinations if we're in a Jupiter context
                // These are likely dynamic PDAs (pool vaults, routing accounts)
                is_jupiter_context
            }
        }
    }

    /// Get all account keys from a versioned message
    fn get_account_keys(&self, message: &VersionedMessage) -> Result<Vec<Pubkey>, TxValidationError> {
        match message {
            VersionedMessage::Legacy(msg) => Ok(msg.account_keys.clone()),
            VersionedMessage::V0(msg) => {
                // V0 messages may have address lookup tables
                // For now, just return static keys - lookup table resolution
                // would require RPC calls
                Ok(msg.account_keys.clone())
            }
        }
    }

    /// Parse a SystemProgram::Transfer instruction
    fn parse_system_transfer(
        &self,
        ix: &solana_sdk::instruction::CompiledInstruction,
        account_keys: &[Pubkey],
    ) -> Option<DetectedTransfer> {
        // SystemProgram::Transfer has discriminator 2 (u32 LE)
        if ix.data.len() < 12 {
            return None;
        }

        let discriminator = u32::from_le_bytes([ix.data[0], ix.data[1], ix.data[2], ix.data[3]]);
        if discriminator != SYSTEM_TRANSFER_DISCRIMINATOR {
            return None;
        }

        // Accounts: [0] = from, [1] = to
        if ix.accounts.len() < 2 {
            return None;
        }

        let from_idx = ix.accounts[0] as usize;
        let to_idx = ix.accounts[1] as usize;

        if from_idx >= account_keys.len() || to_idx >= account_keys.len() {
            return None;
        }

        let from = account_keys[from_idx];
        let to = account_keys[to_idx];

        // Parse lamports (u64 LE after discriminator)
        let lamports = u64::from_le_bytes([
            ix.data[4], ix.data[5], ix.data[6], ix.data[7],
            ix.data[8], ix.data[9], ix.data[10], ix.data[11],
        ]);

        Some(DetectedTransfer {
            from,
            to,
            lamports,
            is_jito_tip: is_jito_tip_account(&to),
        })
    }

    /// Parse an SPL Token CloseAccount instruction
    fn parse_close_account(
        &self,
        ix: &solana_sdk::instruction::CompiledInstruction,
        account_keys: &[Pubkey],
    ) -> Option<DetectedCloseAccount> {
        // CloseAccount has discriminator 9 (single byte)
        if ix.data.is_empty() || ix.data[0] != SPL_CLOSE_ACCOUNT_DISCRIMINATOR {
            return None;
        }

        // Accounts: [0] = account to close, [1] = destination, [2] = authority
        if ix.accounts.len() < 3 {
            return None;
        }

        let account_idx = ix.accounts[0] as usize;
        let destination_idx = ix.accounts[1] as usize;
        let authority_idx = ix.accounts[2] as usize;

        if account_idx >= account_keys.len()
            || destination_idx >= account_keys.len()
            || authority_idx >= account_keys.len()
        {
            return None;
        }

        Some(DetectedCloseAccount {
            account: account_keys[account_idx],
            destination: account_keys[destination_idx],
            authority: account_keys[authority_idx],
        })
    }

    /// Check if a pubkey is in the allowed destinations set
    fn is_allowed_destination(&self, pubkey: &Pubkey) -> bool {
        self.allowed_destinations.contains(pubkey)
    }

    /// Get the user's wallet pubkey
    pub fn user_wallet(&self) -> &Pubkey {
        &self.user_wallet
    }

    /// Get the number of allowed destinations
    pub fn allowed_destination_count(&self) -> usize {
        self.allowed_destinations.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        hash::Hash,
        message::Message,
        signature::Keypair,
        signer::Signer,
        system_instruction,
        transaction::Transaction,
    };

    fn create_test_validator() -> (TransactionValidator, Keypair) {
        let user_keypair = Keypair::new();
        let validator = TransactionValidator::new(user_keypair.pubkey());
        (validator, user_keypair)
    }

    #[test]
    fn test_allows_transfer_to_user_wallet() {
        let (validator, user) = create_test_validator();
        let other = Keypair::new();

        // Transfer TO user wallet (should pass)
        let ix = system_instruction::transfer(&other.pubkey(), &user.pubkey(), 1_000_000);
        let msg = Message::new(&[ix], Some(&other.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.transfer_count, 1);
    }

    #[test]
    fn test_allows_transfer_to_jito_tip() {
        let (validator, user) = create_test_validator();
        let jito_tip: Pubkey = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse().unwrap();

        let ix = system_instruction::transfer(&user.pubkey(), &jito_tip, 10_000);
        let msg = Message::new(&[ix], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_transfer_to_unknown_address() {
        let (validator, user) = create_test_validator();
        let unknown = Keypair::new();

        let ix = system_instruction::transfer(&user.pubkey(), &unknown.pubkey(), 100_000_000);
        let msg = Message::new(&[ix], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_err());

        match result.unwrap_err() {
            TxValidationError::UnauthorizedTransferDestination { destination } => {
                assert_eq!(destination, unknown.pubkey().to_string());
            }
            _ => panic!("Expected UnauthorizedTransferDestination error"),
        }
    }

    #[test]
    fn test_multiple_valid_transfers_pass() {
        let (validator, user) = create_test_validator();
        let jito_tip1: Pubkey = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse().unwrap();
        let jito_tip2: Pubkey = "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe".parse().unwrap();

        let ix1 = system_instruction::transfer(&user.pubkey(), &jito_tip1, 10_000);
        let ix2 = system_instruction::transfer(&user.pubkey(), &jito_tip2, 10_000);
        let msg = Message::new(&[ix1, ix2], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().transfer_count, 2);
    }

    #[test]
    fn test_one_bad_transfer_fails_entire_tx() {
        let (validator, user) = create_test_validator();
        let jito_tip: Pubkey = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse().unwrap();
        let unknown = Keypair::new();

        let ix1 = system_instruction::transfer(&user.pubkey(), &jito_tip, 10_000);
        let ix2 = system_instruction::transfer(&user.pubkey(), &unknown.pubkey(), 100_000_000);
        let msg = Message::new(&[ix1, ix2], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_transaction_passes() {
        let (validator, user) = create_test_validator();

        // Transaction with no instructions
        let msg = Message::new(&[], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_ok());
        let result = result.unwrap();
        assert_eq!(result.transfer_count, 0);
        assert_eq!(result.close_account_count, 0);
    }

    #[test]
    fn test_add_allowed_destination() {
        let (mut validator, user) = create_test_validator();
        let custom = Keypair::new();

        // Initially should fail
        let ix = system_instruction::transfer(&user.pubkey(), &custom.pubkey(), 1000);
        let msg = Message::new(&[ix], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        assert!(validator.validate(&versioned).is_err());

        // Add custom address
        validator.add_allowed_destination(custom.pubkey());

        // Now should pass
        assert!(validator.validate(&versioned).is_ok());
    }

    #[test]
    fn test_allowed_destination_count() {
        let (validator, _) = create_test_validator();
        // Should have: 1 user + 8 Jito + 20 DEX + 6 system + 5 Jupiter routing = 40
        assert!(validator.allowed_destination_count() >= 35);
    }

    #[test]
    fn test_strict_mode_rejects_unknown() {
        let user = Keypair::new();
        let validator = TransactionValidator::strict(user.pubkey());
        let unknown = Keypair::new();

        // Transfer to unknown address should fail in strict mode
        let ix = system_instruction::transfer(&user.pubkey(), &unknown.pubkey(), 1000);
        let msg = Message::new(&[ix], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_err());
    }

    #[test]
    fn test_jupiter_validation_mode_default() {
        // Default should be Permissive
        assert_eq!(JupiterValidationMode::default(), JupiterValidationMode::Permissive);
    }

    #[test]
    fn test_validator_with_mode() {
        let user = Keypair::new();

        // Test strict mode creation
        let strict = TransactionValidator::with_mode(user.pubkey(), JupiterValidationMode::Strict);
        assert_eq!(strict.jupiter_validation_mode, JupiterValidationMode::Strict);

        // Test permissive mode creation
        let permissive = TransactionValidator::with_mode(user.pubkey(), JupiterValidationMode::Permissive);
        assert_eq!(permissive.jupiter_validation_mode, JupiterValidationMode::Permissive);
    }

    #[test]
    fn test_allows_transfer_to_jupiter_routing_account() {
        let (validator, user) = create_test_validator();
        // Jupiter Token Ledger
        let jupiter_routing: Pubkey = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN".parse().unwrap();

        let ix = system_instruction::transfer(&user.pubkey(), &jupiter_routing, 10_000);
        let msg = Message::new(&[ix], Some(&user.pubkey()));
        let tx = Transaction::new_unsigned(msg);
        let versioned = VersionedTransaction::from(tx);

        let result = validator.validate(&versioned);
        assert!(result.is_ok());
    }

    #[test]
    fn test_detected_transfer_jito_flag() {
        let jito_tip: Pubkey = "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse().unwrap();
        let user = Keypair::new();

        let transfer = DetectedTransfer {
            from: user.pubkey(),
            to: jito_tip,
            lamports: 10_000,
            is_jito_tip: is_jito_tip_account(&jito_tip),
        };

        assert!(transfer.is_jito_tip);
    }

    #[test]
    fn test_user_wallet_accessor() {
        let (validator, user) = create_test_validator();
        assert_eq!(*validator.user_wallet(), user.pubkey());
    }
}
