//! Known Program Addresses
//!
//! Constants for known Solana programs, DEX addresses, and Jito tip accounts.
//! Used by the transaction validator to build the destination allowlist.

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Jito validator tip accounts (8 official accounts)
pub const JITO_TIP_ACCOUNTS: &[&str] = &[
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

/// Known DEX program IDs that are safe to interact with
pub const KNOWN_DEX_PROGRAMS: &[&str] = &[
    // Jupiter Aggregator v6
    "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4",
    // Jupiter Limit Order
    "jupoNjAxXgZ4rjzxzPMP4oxduvQsQtZzyknqvzYNrNu",
    // Jupiter Limit Order v2
    "j1o2qRpjcyUwEvwtcfhS9NCHT98wiXpESLWnqPN62Cu",
    // Jupiter DCA (Dollar Cost Average)
    "DCA265Vj8a9CEuX1eb1LWRnDT7uK6q1xMipnNyatn23M",
    // Jupiter Perpetuals
    "PERPHjGBqRHArX4DySjwM6UJHiR3sWAatqfdBS2qQJu",
    // Raydium AMM v4
    "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8",
    // Raydium CLMM
    "CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK",
    // Raydium CP (Constant Product)
    "CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C",
    // Orca Whirlpool
    "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc",
    // Meteora DLMM
    "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo",
    // Meteora Pools
    "Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB",
    // Phoenix DEX
    "PhoeNiXZ8ByJGLkxNfZRnkUfjvmuYqLR89jjFHGqdXY",
    // OpenBook v2
    "opnb2LAfJYbRMAHHvqjCwQxanZn7ReEHp1k81EohpZb",
    // Lifinity v2
    "2wT8Yq49kHgDzXuPxZSaeLaH1qbmGXtEyPy64bL7aD3c",
    // Marinade Finance
    "MarBmsSgKXdrN1egZf5sqe1TMai9K1rChYNDJgjq7aD",
    // Sanctum (Infinity)
    "5ocnV1qiCgaQR8Jb8xWnVbApfaygJ8tNoZfgPwsgx9kx",
    // FluxBeam
    "FLUXubRmkEi2q6K3Y9kBPg9248ggaZVsoSFhtJHSrm1X",
];

/// System programs that are always allowed
pub const SYSTEM_PROGRAMS: &[&str] = &[
    // System Program
    "11111111111111111111111111111111",
    // SPL Token Program
    "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
    // SPL Token 2022
    "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb",
    // Associated Token Account Program
    "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
    // Compute Budget Program
    "ComputeBudget111111111111111111111111111111",
    // Address Lookup Table Program
    "AddressLookupTab1e1111111111111111111111111",
];

/// Jupiter routing accounts and fee/referral accounts
/// These are intermediate accounts used by Jupiter for routing swaps
pub const JUPITER_ROUTING_ACCOUNTS: &[&str] = &[
    // Jupiter referral fee account / routing pool vault
    "BuFNtMZG6SpfAcwZxzRWF5N3XdaG3XoUCZZLZxbdm27b",
    // Jupiter Token Ledger (shared accounts for routes)
    "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN",
    // Jupiter Referral Program
    "REFER4ZgmyYx9c6He5XfaTMiGfdLwRnkV4RPp9t9iF3",
    // Jupiter v6 Event Authority
    "D8cy77BBepLMngZx6ZukaTff5hCt1HrWyKk3Hnd9oitf",
    // Common Jupiter fee account
    "45ruCyfdRkWpRNGEqWzjCiXRHkZs8WXCLQ67Pnpye7Hp",
];

/// Native SOL mint (wrapped SOL)
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";

/// Parse all Jito tip accounts into Pubkeys
pub fn jito_tip_pubkeys() -> Vec<Pubkey> {
    JITO_TIP_ACCOUNTS
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect()
}

/// Parse all known DEX program IDs into Pubkeys
pub fn dex_program_pubkeys() -> Vec<Pubkey> {
    KNOWN_DEX_PROGRAMS
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect()
}

/// Parse all system program IDs into Pubkeys
pub fn system_program_pubkeys() -> Vec<Pubkey> {
    SYSTEM_PROGRAMS
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect()
}

/// Parse all Jupiter routing accounts into Pubkeys
pub fn jupiter_routing_pubkeys() -> Vec<Pubkey> {
    JUPITER_ROUTING_ACCOUNTS
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect()
}

/// Check if a pubkey is a known Jito tip account
pub fn is_jito_tip_account(pubkey: &Pubkey) -> bool {
    let pubkey_str = pubkey.to_string();
    JITO_TIP_ACCOUNTS.contains(&pubkey_str.as_str())
}

/// Check if a pubkey is a known DEX program
pub fn is_known_dex_program(pubkey: &Pubkey) -> bool {
    let pubkey_str = pubkey.to_string();
    KNOWN_DEX_PROGRAMS.contains(&pubkey_str.as_str())
}

/// Check if a pubkey is a system program
pub fn is_system_program(pubkey: &Pubkey) -> bool {
    let pubkey_str = pubkey.to_string();
    SYSTEM_PROGRAMS.contains(&pubkey_str.as_str())
}

/// Check if a pubkey is a known Jupiter routing account (fee/referral accounts)
pub fn is_jupiter_routing_account(pubkey: &Pubkey) -> bool {
    let pubkey_str = pubkey.to_string();
    JUPITER_ROUTING_ACCOUNTS.contains(&pubkey_str.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jito_tip_pubkeys_parse() {
        let pubkeys = jito_tip_pubkeys();
        assert_eq!(pubkeys.len(), 8);
    }

    #[test]
    fn test_dex_program_pubkeys_parse() {
        let pubkeys = dex_program_pubkeys();
        assert_eq!(pubkeys.len(), 17);
    }

    #[test]
    fn test_system_program_pubkeys_parse() {
        let pubkeys = system_program_pubkeys();
        assert_eq!(pubkeys.len(), 6);
    }

    #[test]
    fn test_is_jito_tip_account() {
        let tip = Pubkey::from_str("96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5").unwrap();
        assert!(is_jito_tip_account(&tip));

        let not_tip = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        assert!(!is_jito_tip_account(&not_tip));
    }

    #[test]
    fn test_is_known_dex_program() {
        let jupiter = Pubkey::from_str("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4").unwrap();
        assert!(is_known_dex_program(&jupiter));
    }

    #[test]
    fn test_is_system_program() {
        let system = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        assert!(is_system_program(&system));

        let token = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        assert!(is_system_program(&token));
    }

    #[test]
    fn test_jupiter_routing_pubkeys_parse() {
        let pubkeys = jupiter_routing_pubkeys();
        assert_eq!(pubkeys.len(), 5);
    }

    #[test]
    fn test_is_jupiter_routing_account() {
        let routing = Pubkey::from_str("BuFNtMZG6SpfAcwZxzRWF5N3XdaG3XoUCZZLZxbdm27b").unwrap();
        assert!(is_jupiter_routing_account(&routing));

        let not_routing = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        assert!(!is_jupiter_routing_account(&not_routing));
    }
}
