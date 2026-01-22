//! Token-2022 Extension Parsing
//!
//! Parses Token-2022 mint account data to detect extensions.
//! Based on SPL Token-2022 TLV (Type-Length-Value) format.
//!
//! Standard Mint Account Layout (first 82 bytes):
//! - Offset 0-3:   mint_authority_option (u32: 0=None, 1=Some)
//! - Offset 4-35:  mint_authority (Pubkey, 32 bytes) - if option=1
//! - Offset 36-43: supply (u64)
//! - Offset 44:    decimals (u8)
//! - Offset 45:    is_initialized (bool)
//! - Offset 46-49: freeze_authority_option (u32: 0=None, 1=Some)
//! - Offset 50-81: freeze_authority (Pubkey, 32 bytes) - if option=1
//!
//! Token-2022 extensions start after byte 82 (base mint size) + alignment padding.

use solana_sdk::pubkey::Pubkey;
use std::convert::TryFrom;

/// Standard SPL Token program ID
pub const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
/// Token-2022 program ID
pub const TOKEN_2022_PROGRAM_ID: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

/// Base mint account size (standard fields)
pub const MINT_BASE_SIZE: usize = 82;
/// Account type discriminator offset for Token-2022
pub const ACCOUNT_TYPE_OFFSET: usize = 82;
/// Extensions start offset (after base + account type + padding)
pub const EXTENSIONS_START_OFFSET: usize = 165;

/// Token program type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenProgram {
    /// Standard SPL Token program
    Spl,
    /// Token-2022 program with extensions
    Token2022,
}

/// Validate the token program and return its type
pub fn validate_token_program(owner: &Pubkey) -> Result<TokenProgram, TokenProgramError> {
    let owner_str = owner.to_string();
    match owner_str.as_str() {
        SPL_TOKEN_PROGRAM_ID => Ok(TokenProgram::Spl),
        TOKEN_2022_PROGRAM_ID => Ok(TokenProgram::Token2022),
        other => Err(TokenProgramError::UnknownProgram {
            program: other.to_string(),
        }),
    }
}

/// Error type for token program validation
#[derive(Debug, Clone)]
pub enum TokenProgramError {
    /// Unknown token program - could have arbitrary transfer logic
    UnknownProgram { program: String },
    /// Data too short to parse
    DataTooShort { expected: usize, actual: usize },
    /// Invalid extension data
    InvalidExtension { extension_type: u16, reason: String },
}

impl std::fmt::Display for TokenProgramError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenProgramError::UnknownProgram { program } => {
                write!(f, "Unknown token program: {} - could have arbitrary transfer logic", program)
            }
            TokenProgramError::DataTooShort { expected, actual } => {
                write!(f, "Data too short: expected {} bytes, got {}", expected, actual)
            }
            TokenProgramError::InvalidExtension { extension_type, reason } => {
                write!(f, "Invalid extension type {}: {}", extension_type, reason)
            }
        }
    }
}

impl std::error::Error for TokenProgramError {}

/// Authority information from mint account
#[derive(Debug, Clone, Default)]
pub struct AuthorityInfo {
    /// Mint authority (can mint new tokens)
    pub mint_authority: Option<Pubkey>,
    /// Freeze authority (can freeze token accounts)
    pub freeze_authority: Option<Pubkey>,
}

/// Parse mint and freeze authorities from mint account data
///
/// Works for both SPL Token and Token-2022 mints.
pub fn parse_authorities(mint_data: &[u8]) -> Result<AuthorityInfo, TokenProgramError> {
    if mint_data.len() < MINT_BASE_SIZE {
        return Err(TokenProgramError::DataTooShort {
            expected: MINT_BASE_SIZE,
            actual: mint_data.len(),
        });
    }

    // Parse mint authority (offset 0-35)
    let mint_auth_option = u32::from_le_bytes(
        mint_data[0..4]
            .try_into()
            .map_err(|_| TokenProgramError::DataTooShort {
                expected: 4,
                actual: mint_data.len(),
            })?,
    );

    let mint_authority = if mint_auth_option == 1 {
        Some(
            Pubkey::try_from(&mint_data[4..36])
                .map_err(|_| TokenProgramError::DataTooShort {
                    expected: 36,
                    actual: mint_data.len(),
                })?,
        )
    } else {
        None
    };

    // Parse freeze authority (offset 46-81)
    let freeze_auth_option = u32::from_le_bytes(
        mint_data[46..50]
            .try_into()
            .map_err(|_| TokenProgramError::DataTooShort {
                expected: 50,
                actual: mint_data.len(),
            })?,
    );

    let freeze_authority = if freeze_auth_option == 1 {
        Some(
            Pubkey::try_from(&mint_data[50..82])
                .map_err(|_| TokenProgramError::DataTooShort {
                    expected: 82,
                    actual: mint_data.len(),
                })?,
        )
    } else {
        None
    };

    Ok(AuthorityInfo {
        mint_authority,
        freeze_authority,
    })
}

/// Token-2022 extension types
///
/// Based on SPL Token-2022 specification.
/// Not all types are dangerous - we flag the ones that can affect transferability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum ExtensionType {
    /// Uninitialized extension slot
    Uninitialized = 0,
    /// Transfer fee configuration
    TransferFeeConfig = 1,
    /// Transfer fee state
    TransferFeeAmount = 2,
    /// Mint close authority
    MintCloseAuthority = 3,
    /// Confidential transfer mint
    ConfidentialTransferMint = 4,
    /// Confidential transfer account
    ConfidentialTransferAccount = 5,
    /// Default account state (e.g., frozen by default)
    DefaultAccountState = 6,
    /// Immutable owner
    ImmutableOwner = 7,
    /// Memo required on transfer
    MemoTransfer = 8,
    /// Non-transferable (soulbound)
    NonTransferable = 9,
    /// Interest-bearing config
    InterestBearingConfig = 10,
    /// CPI guard
    CpiGuard = 11,
    /// Permanent delegate
    PermanentDelegate = 12,
    /// Non-transferable account (state)
    NonTransferableAccount = 13,
    /// Transfer hook
    TransferHook = 14,
    /// Transfer hook account
    TransferHookAccount = 15,
    /// Confidential transfer fee config
    ConfidentialTransferFeeConfig = 16,
    /// Confidential transfer fee amount
    ConfidentialTransferFeeAmount = 17,
    /// Metadata pointer
    MetadataPointer = 18,
    /// Token metadata
    TokenMetadata = 19,
    /// Group pointer
    GroupPointer = 20,
    /// Token group
    TokenGroup = 21,
    /// Group member pointer
    GroupMemberPointer = 22,
    /// Token group member
    TokenGroupMember = 23,
    /// Confidential mint/burn
    ConfidentialMintBurn = 24,
    /// Scaled UI amount
    ScaledUiAmountConfig = 25,
    /// Pausable
    Pausable = 26,
    /// Pausable account
    PausableAccount = 27,
    /// Unknown extension type
    Unknown(u16),
}

impl From<u16> for ExtensionType {
    fn from(value: u16) -> Self {
        match value {
            0 => ExtensionType::Uninitialized,
            1 => ExtensionType::TransferFeeConfig,
            2 => ExtensionType::TransferFeeAmount,
            3 => ExtensionType::MintCloseAuthority,
            4 => ExtensionType::ConfidentialTransferMint,
            5 => ExtensionType::ConfidentialTransferAccount,
            6 => ExtensionType::DefaultAccountState,
            7 => ExtensionType::ImmutableOwner,
            8 => ExtensionType::MemoTransfer,
            9 => ExtensionType::NonTransferable,
            10 => ExtensionType::InterestBearingConfig,
            11 => ExtensionType::CpiGuard,
            12 => ExtensionType::PermanentDelegate,
            13 => ExtensionType::NonTransferableAccount,
            14 => ExtensionType::TransferHook,
            15 => ExtensionType::TransferHookAccount,
            16 => ExtensionType::ConfidentialTransferFeeConfig,
            17 => ExtensionType::ConfidentialTransferFeeAmount,
            18 => ExtensionType::MetadataPointer,
            19 => ExtensionType::TokenMetadata,
            20 => ExtensionType::GroupPointer,
            21 => ExtensionType::TokenGroup,
            22 => ExtensionType::GroupMemberPointer,
            23 => ExtensionType::TokenGroupMember,
            24 => ExtensionType::ConfidentialMintBurn,
            25 => ExtensionType::ScaledUiAmountConfig,
            26 => ExtensionType::Pausable,
            27 => ExtensionType::PausableAccount,
            other => ExtensionType::Unknown(other),
        }
    }
}

impl ExtensionType {
    /// Whether this extension is considered dangerous for trading
    pub fn is_dangerous(&self) -> bool {
        matches!(
            self,
            ExtensionType::PermanentDelegate
                | ExtensionType::NonTransferable
                | ExtensionType::NonTransferableAccount
                | ExtensionType::Pausable
                | ExtensionType::PausableAccount
        )
    }

    /// Whether this extension needs additional checks
    pub fn needs_review(&self) -> bool {
        matches!(
            self,
            ExtensionType::TransferHook
                | ExtensionType::TransferHookAccount
                | ExtensionType::DefaultAccountState
                | ExtensionType::TransferFeeConfig
                | ExtensionType::ConfidentialTransferMint
        )
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            ExtensionType::Uninitialized => "Uninitialized",
            ExtensionType::TransferFeeConfig => "TransferFeeConfig",
            ExtensionType::TransferFeeAmount => "TransferFeeAmount",
            ExtensionType::MintCloseAuthority => "MintCloseAuthority",
            ExtensionType::ConfidentialTransferMint => "ConfidentialTransferMint",
            ExtensionType::ConfidentialTransferAccount => "ConfidentialTransferAccount",
            ExtensionType::DefaultAccountState => "DefaultAccountState",
            ExtensionType::ImmutableOwner => "ImmutableOwner",
            ExtensionType::MemoTransfer => "MemoTransfer",
            ExtensionType::NonTransferable => "NonTransferable",
            ExtensionType::InterestBearingConfig => "InterestBearingConfig",
            ExtensionType::CpiGuard => "CpiGuard",
            ExtensionType::PermanentDelegate => "PermanentDelegate",
            ExtensionType::NonTransferableAccount => "NonTransferableAccount",
            ExtensionType::TransferHook => "TransferHook",
            ExtensionType::TransferHookAccount => "TransferHookAccount",
            ExtensionType::ConfidentialTransferFeeConfig => "ConfidentialTransferFeeConfig",
            ExtensionType::ConfidentialTransferFeeAmount => "ConfidentialTransferFeeAmount",
            ExtensionType::MetadataPointer => "MetadataPointer",
            ExtensionType::TokenMetadata => "TokenMetadata",
            ExtensionType::GroupPointer => "GroupPointer",
            ExtensionType::TokenGroup => "TokenGroup",
            ExtensionType::GroupMemberPointer => "GroupMemberPointer",
            ExtensionType::TokenGroupMember => "TokenGroupMember",
            ExtensionType::ConfidentialMintBurn => "ConfidentialMintBurn",
            ExtensionType::ScaledUiAmountConfig => "ScaledUiAmountConfig",
            ExtensionType::Pausable => "Pausable",
            ExtensionType::PausableAccount => "PausableAccount",
            ExtensionType::Unknown(_) => "Unknown",
        }
    }
}

/// Parsed extension with additional data
#[derive(Debug, Clone)]
pub struct ParsedExtension {
    pub extension_type: ExtensionType,
    /// Raw extension data (excluding type and length)
    pub data: Vec<u8>,
    /// Parsed transfer hook program ID if applicable
    pub transfer_hook_program: Option<Pubkey>,
    /// Parsed transfer fee in basis points if applicable
    pub transfer_fee_bps: Option<u16>,
    /// Parsed permanent delegate if applicable
    pub permanent_delegate: Option<Pubkey>,
    /// Default account state if applicable (1=Frozen, 2=Initialized)
    pub default_account_state: Option<u8>,
}

impl ParsedExtension {
    fn new(extension_type: ExtensionType, data: Vec<u8>) -> Self {
        Self {
            extension_type,
            data,
            transfer_hook_program: None,
            transfer_fee_bps: None,
            permanent_delegate: None,
            default_account_state: None,
        }
    }
}

/// Parse Token-2022 extensions from mint account data
///
/// Returns a list of detected extensions. Only valid for Token-2022 mints.
pub fn parse_token2022_extensions(mint_data: &[u8]) -> Result<Vec<ParsedExtension>, TokenProgramError> {
    // Token-2022 mints have additional data after the base mint fields
    if mint_data.len() <= MINT_BASE_SIZE {
        // No extensions
        return Ok(Vec::new());
    }

    // Check account type at offset 82 (should be 1 for Mint in Token-2022)
    if mint_data.len() > ACCOUNT_TYPE_OFFSET {
        let account_type = mint_data[ACCOUNT_TYPE_OFFSET];
        if account_type != 1 {
            // Not a Token-2022 mint account type
            return Ok(Vec::new());
        }
    }

    let mut extensions = Vec::new();

    // Parse TLV extensions starting after base + account type + padding
    // Token-2022 uses variable offsets, we need to scan from the right location
    let mut offset = EXTENSIONS_START_OFFSET;

    while offset + 4 <= mint_data.len() {
        // Read extension type (2 bytes, little endian)
        let ext_type_raw = u16::from_le_bytes([mint_data[offset], mint_data[offset + 1]]);

        // Read extension length (2 bytes, little endian)
        let ext_length = u16::from_le_bytes([mint_data[offset + 2], mint_data[offset + 3]]) as usize;

        // Stop if we hit uninitialized or the length would exceed data
        if ext_type_raw == 0 || offset + 4 + ext_length > mint_data.len() {
            break;
        }

        let ext_type = ExtensionType::from(ext_type_raw);
        let ext_data = mint_data[offset + 4..offset + 4 + ext_length].to_vec();

        let mut parsed = ParsedExtension::new(ext_type, ext_data.clone());

        // Parse specific extension data
        match ext_type {
            ExtensionType::TransferHook => {
                // TransferHook extension: authority (32) + program_id (32)
                if ext_data.len() >= 64 {
                    if let Ok(program_id) = Pubkey::try_from(&ext_data[32..64]) {
                        // Only set if not the default (all zeros)
                        if program_id != Pubkey::default() {
                            parsed.transfer_hook_program = Some(program_id);
                        }
                    }
                }
            }
            ExtensionType::TransferFeeConfig => {
                // TransferFeeConfig: complex structure, extract current fee
                // Layout: transfer_fee_config_authority (36) + withdraw_withheld_authority (36) +
                //         withheld_amount (8) + older_transfer_fee (16) + newer_transfer_fee (16)
                // Transfer fee has: epoch (8) + maximum_fee (8) + transfer_fee_basis_points (2)
                if ext_data.len() >= 96 {
                    // Get newer transfer fee basis points (offset 88-90 in the extension)
                    let fee_bps = u16::from_le_bytes([ext_data[94], ext_data[95]]);
                    parsed.transfer_fee_bps = Some(fee_bps);
                }
            }
            ExtensionType::PermanentDelegate => {
                // PermanentDelegate: delegate pubkey (32 bytes)
                if ext_data.len() >= 32 {
                    if let Ok(delegate) = Pubkey::try_from(&ext_data[0..32]) {
                        if delegate != Pubkey::default() {
                            parsed.permanent_delegate = Some(delegate);
                        }
                    }
                }
            }
            ExtensionType::DefaultAccountState => {
                // DefaultAccountState: state (1 byte)
                // 0 = Uninitialized, 1 = Initialized, 2 = Frozen
                if !ext_data.is_empty() {
                    parsed.default_account_state = Some(ext_data[0]);
                }
            }
            _ => {}
        }

        extensions.push(parsed);

        // Move to next extension
        offset += 4 + ext_length;

        // Align to 8 bytes (Token-2022 requirement)
        let remainder = offset % 8;
        if remainder != 0 {
            offset += 8 - remainder;
        }
    }

    Ok(extensions)
}

/// Check if any dangerous extensions are present
pub fn has_dangerous_extensions(extensions: &[ParsedExtension]) -> bool {
    extensions.iter().any(|e| e.extension_type.is_dangerous())
}

/// Get list of dangerous extensions
pub fn get_dangerous_extensions(extensions: &[ParsedExtension]) -> Vec<&ParsedExtension> {
    extensions
        .iter()
        .filter(|e| e.extension_type.is_dangerous())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_spl_token() {
        let spl_token =
            Pubkey::try_from(bs58::decode(SPL_TOKEN_PROGRAM_ID).into_vec().unwrap().as_slice())
                .unwrap();
        assert_eq!(validate_token_program(&spl_token).unwrap(), TokenProgram::Spl);
    }

    #[test]
    fn test_validate_token_2022() {
        let token_2022 =
            Pubkey::try_from(bs58::decode(TOKEN_2022_PROGRAM_ID).into_vec().unwrap().as_slice())
                .unwrap();
        assert_eq!(
            validate_token_program(&token_2022).unwrap(),
            TokenProgram::Token2022
        );
    }

    #[test]
    fn test_validate_unknown_program() {
        let unknown = Pubkey::new_unique();
        assert!(validate_token_program(&unknown).is_err());
    }

    #[test]
    fn test_parse_authorities_no_authority() {
        // Create minimal mint data with no authorities
        let mut mint_data = vec![0u8; 82];
        // mint_authority_option = 0 (None)
        mint_data[0..4].copy_from_slice(&0u32.to_le_bytes());
        // freeze_authority_option = 0 (None)
        mint_data[46..50].copy_from_slice(&0u32.to_le_bytes());

        let auth = parse_authorities(&mint_data).unwrap();
        assert!(auth.mint_authority.is_none());
        assert!(auth.freeze_authority.is_none());
    }

    #[test]
    fn test_parse_authorities_with_both() {
        let mut mint_data = vec![0u8; 82];

        // Set mint authority
        mint_data[0..4].copy_from_slice(&1u32.to_le_bytes());
        let mint_auth = Pubkey::new_unique();
        mint_data[4..36].copy_from_slice(mint_auth.as_ref());

        // Set freeze authority
        mint_data[46..50].copy_from_slice(&1u32.to_le_bytes());
        let freeze_auth = Pubkey::new_unique();
        mint_data[50..82].copy_from_slice(freeze_auth.as_ref());

        let auth = parse_authorities(&mint_data).unwrap();
        assert_eq!(auth.mint_authority, Some(mint_auth));
        assert_eq!(auth.freeze_authority, Some(freeze_auth));
    }

    #[test]
    fn test_parse_authorities_data_too_short() {
        let mint_data = vec![0u8; 50]; // Too short
        assert!(parse_authorities(&mint_data).is_err());
    }

    #[test]
    fn test_extension_type_dangerous() {
        assert!(ExtensionType::PermanentDelegate.is_dangerous());
        assert!(ExtensionType::NonTransferable.is_dangerous());
        assert!(ExtensionType::Pausable.is_dangerous());
        assert!(!ExtensionType::TransferFeeConfig.is_dangerous());
        assert!(!ExtensionType::MetadataPointer.is_dangerous());
    }

    #[test]
    fn test_extension_type_needs_review() {
        assert!(ExtensionType::TransferHook.needs_review());
        assert!(ExtensionType::DefaultAccountState.needs_review());
        assert!(ExtensionType::TransferFeeConfig.needs_review());
        assert!(!ExtensionType::MetadataPointer.needs_review());
    }

    #[test]
    fn test_extension_type_from_u16() {
        assert_eq!(ExtensionType::from(1), ExtensionType::TransferFeeConfig);
        assert_eq!(ExtensionType::from(12), ExtensionType::PermanentDelegate);
        assert_eq!(ExtensionType::from(14), ExtensionType::TransferHook);
        assert_eq!(ExtensionType::from(9999), ExtensionType::Unknown(9999));
    }

    #[test]
    fn test_parse_empty_extensions() {
        // Standard SPL Token mint (no extensions)
        let mint_data = vec![0u8; 82];
        let extensions = parse_token2022_extensions(&mint_data).unwrap();
        assert!(extensions.is_empty());
    }

    #[test]
    fn test_extension_names() {
        assert_eq!(ExtensionType::TransferHook.name(), "TransferHook");
        assert_eq!(ExtensionType::PermanentDelegate.name(), "PermanentDelegate");
        assert_eq!(ExtensionType::Unknown(999).name(), "Unknown");
    }
}
