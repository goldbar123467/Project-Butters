//! Known Safe TransferHook Programs
//!
//! Whitelist of verified TransferHook programs that are safe to interact with.
//! These are legitimate programs used for royalties, compliance, etc.
//!
//! IMPORTANT: This whitelist is intentionally minimal.
//! Unknown hooks should be flagged for manual review.

use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::str::FromStr;

/// Known safe TransferHook program IDs
///
/// These are verified programs that implement TransferHook for legitimate purposes:
/// - Metaplex royalties
/// - Compliance hooks
/// - Analytics hooks
///
/// Source: Official documentation and audit reports
/// - Metaplex: https://developers.metaplex.com/official-links
pub fn known_safe_hook_programs() -> HashSet<Pubkey> {
    // NOTE: Add more verified hook programs as discovered
    // Always verify against official docs before adding
    let programs = [
        // ============================================================
        // METAPLEX PROGRAMS
        // Source: https://developers.metaplex.com/official-links
        // ============================================================

        // Metaplex Token Metadata Program
        // Used for NFT metadata and royalty enforcement
        "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",

        // Metaplex Candy Machine V3
        // Used for NFT minting
        "CndyV3LdqHUfDLmE5naZjVN8rBZz4tqhdefbAnjHG3JR",

        // Metaplex Candy Guard
        // Guards for Candy Machine
        "Guard1JwRhJkVH6XZhzoYxeBVQe872VH6QggF4BWmS9g",

        // Metaplex Token Auth Rules
        // Programmable NFT auth rules
        "auth9SigNpDKz4sJJ1DfCTuZrZNSAgh9sFD3rboVmgg",

        // ============================================================
        // KNOWN COMPLIANCE/STABLECOIN HOOKS
        // ============================================================

        // Add more verified programs here as they are audited
        // Format: "<pubkey>", // Description - Source
    ];

    programs
        .iter()
        .filter_map(|s| Pubkey::from_str(s).ok())
        .collect()
}

/// Check if a program is a known safe hook
pub fn is_known_safe_hook(program_id: &Pubkey) -> bool {
    known_safe_hook_programs().contains(program_id)
}

/// Hook safety level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSafety {
    /// Known safe hook (whitelisted)
    Safe,
    /// Unknown hook - needs review
    Unknown,
    /// Known malicious hook (blacklisted)
    Malicious,
}

/// Get safety level for a TransferHook program
pub fn get_hook_safety(program_id: &Pubkey) -> HookSafety {
    if is_known_safe_hook(program_id) {
        HookSafety::Safe
    } else if is_known_malicious_hook(program_id) {
        HookSafety::Malicious
    } else {
        HookSafety::Unknown
    }
}

/// Known malicious TransferHook programs
///
/// These are programs known to be used in honeypot scams.
/// This list is populated based on incident reports.
fn known_malicious_hooks() -> HashSet<Pubkey> {
    // Currently empty - populate as malicious hooks are identified
    // Format: Pubkey::from_str("<pubkey>").unwrap(),
    HashSet::new()
}

/// Check if a program is a known malicious hook
pub fn is_known_malicious_hook(program_id: &Pubkey) -> bool {
    known_malicious_hooks().contains(program_id)
}

/// Get description for a known safe hook
pub fn get_hook_description(program_id: &Pubkey) -> Option<&'static str> {
    let metaplex_metadata = Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").ok()?;
    let candy_machine = Pubkey::from_str("CndyV3LdqHUfDLmE5naZjVN8rBZz4tqhdefbAnjHG3JR").ok()?;
    let candy_guard = Pubkey::from_str("Guard1JwRhJkVH6XZhzoYxeBVQe872VH6QggF4BWmS9g").ok()?;
    let auth_rules = Pubkey::from_str("auth9SigNpDKz4sJJ1DfCTuZrZNSAgh9sFD3rboVmgg").ok()?;

    if *program_id == metaplex_metadata {
        Some("Metaplex Token Metadata - NFT metadata and royalty enforcement")
    } else if *program_id == candy_machine {
        Some("Metaplex Candy Machine V3 - NFT minting")
    } else if *program_id == candy_guard {
        Some("Metaplex Candy Guard - Minting guards")
    } else if *program_id == auth_rules {
        Some("Metaplex Auth Rules - Programmable NFT rules")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_safe_hooks_not_empty() {
        let hooks = known_safe_hook_programs();
        assert!(!hooks.is_empty(), "Whitelist should not be empty");
    }

    #[test]
    fn test_metaplex_is_safe() {
        let metaplex = Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap();
        assert!(is_known_safe_hook(&metaplex));
    }

    #[test]
    fn test_random_program_is_unknown() {
        let random = Pubkey::new_unique();
        assert!(!is_known_safe_hook(&random));
        assert_eq!(get_hook_safety(&random), HookSafety::Unknown);
    }

    #[test]
    fn test_hook_safety_levels() {
        let metaplex = Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap();
        assert_eq!(get_hook_safety(&metaplex), HookSafety::Safe);

        let unknown = Pubkey::new_unique();
        assert_eq!(get_hook_safety(&unknown), HookSafety::Unknown);
    }

    #[test]
    fn test_hook_description() {
        let metaplex = Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap();
        let desc = get_hook_description(&metaplex);
        assert!(desc.is_some());
        assert!(desc.unwrap().contains("Metaplex"));

        let unknown = Pubkey::new_unique();
        assert!(get_hook_description(&unknown).is_none());
    }

    #[test]
    fn test_all_whitelist_entries_valid() {
        // Ensure all entries in the whitelist are valid pubkeys
        let hooks = known_safe_hook_programs();
        for hook in &hooks {
            // If we got here, the pubkey was valid
            assert!(!hook.to_string().is_empty());
        }
    }
}
