//! Solana Honeypot Detector
//!
//! Production implementation of HoneypotDetector trait for Solana tokens.
//! Checks Token-2022 extensions, authorities, and simulates sells via Jupiter.

use async_trait::async_trait;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::adapters::jupiter::JupiterClient;
use crate::adapters::solana::rpc::SolanaClient;
use crate::domain::honeypot_detector::{
    HoneypotAnalysis, HoneypotDetector, HoneypotDetectorConfig, HoneypotError, HoneypotRisk,
    SimulationResult,
};

use super::cache::HoneypotCache;
use super::known_hooks::{get_hook_safety, HookSafety};
use super::sell_simulator::SellSimulator;
use super::token2022::{
    parse_authorities, parse_token2022_extensions, validate_token_program, ExtensionType,
    TokenProgram,
};

/// Configuration for SolanaHoneypotDetector
#[derive(Debug, Clone)]
pub struct SolanaHoneypotDetectorConfig {
    /// Base honeypot detector config
    pub base: HoneypotDetectorConfig,
    /// Default sell simulation amount (in token base units)
    pub default_simulation_amount: u64,
    /// Enable caching
    pub enable_cache: bool,
    /// Maximum price impact percent before flagging (default: 25%)
    pub max_price_impact_pct: f64,
}

impl Default for SolanaHoneypotDetectorConfig {
    fn default() -> Self {
        Self {
            base: HoneypotDetectorConfig::default(),
            default_simulation_amount: 1_000_000_000, // 1 token with 9 decimals
            enable_cache: true,
            max_price_impact_pct: 25.0,
        }
    }
}

impl SolanaHoneypotDetectorConfig {
    /// Strict configuration - more aggressive blocking
    pub fn strict() -> Self {
        Self {
            base: HoneypotDetectorConfig::strict(),
            default_simulation_amount: 10_000_000_000, // 10 tokens
            enable_cache: true,
            max_price_impact_pct: 15.0,
        }
    }
}

/// Production honeypot detector for Solana tokens
pub struct SolanaHoneypotDetector {
    solana: SolanaClient,
    sell_simulator: SellSimulator,
    config: SolanaHoneypotDetectorConfig,
    cache: Arc<RwLock<HoneypotCache>>,
}

impl SolanaHoneypotDetector {
    /// Create a new honeypot detector
    pub fn new(solana: SolanaClient, jupiter: JupiterClient) -> Self {
        Self::with_config(solana, jupiter, SolanaHoneypotDetectorConfig::default())
    }

    /// Create a new honeypot detector with custom config
    pub fn with_config(
        solana: SolanaClient,
        jupiter: JupiterClient,
        config: SolanaHoneypotDetectorConfig,
    ) -> Self {
        let sell_simulator = SellSimulator::new(jupiter);
        let cache = Arc::new(RwLock::new(HoneypotCache::new()));

        Self {
            solana,
            sell_simulator,
            config,
            cache,
        }
    }

    /// Create a strict honeypot detector
    pub fn strict(solana: SolanaClient, jupiter: JupiterClient) -> Self {
        Self::with_config(solana, jupiter, SolanaHoneypotDetectorConfig::strict())
    }

    /// Fetch mint account data from RPC
    async fn fetch_mint_account(&self, mint: &Pubkey) -> Result<(Pubkey, Vec<u8>), HoneypotError> {
        let client = self.solana.get_rpc_client();
        let mint_pubkey = *mint;

        let account = tokio::task::spawn_blocking(move || {
            client.get_account(&mint_pubkey)
        })
        .await
        .map_err(|e| HoneypotError::RpcError(format!("Task join error: {}", e)))?
        .map_err(|e| {
            if e.to_string().contains("AccountNotFound") {
                HoneypotError::TokenNotFound(mint.to_string())
            } else {
                HoneypotError::RpcError(e.to_string())
            }
        })?;

        Ok((account.owner, account.data))
    }

    /// Analyze token-2022 extensions and build risk assessment
    fn analyze_extensions(
        &self,
        mint_data: &[u8],
        analysis: &mut HoneypotAnalysis,
    ) -> Result<(), HoneypotError> {
        let extensions = parse_token2022_extensions(mint_data).map_err(|e| {
            HoneypotError::SimulationFailed {
                reason: format!("Failed to parse extensions: {}", e),
            }
        })?;

        for ext in &extensions {
            match ext.extension_type {
                ExtensionType::PermanentDelegate => {
                    analysis.has_permanent_delegate = true;
                    if let Some(ref delegate) = ext.permanent_delegate {
                        analysis.issues.push(format!(
                            "CRITICAL: PermanentDelegate set to {} - can steal tokens",
                            delegate
                        ));
                    } else {
                        analysis.issues.push(
                            "CRITICAL: PermanentDelegate extension present".to_string(),
                        );
                    }
                }

                ExtensionType::NonTransferable | ExtensionType::NonTransferableAccount => {
                    analysis.can_transfer = false;
                    analysis.issues.push(
                        "CRITICAL: NonTransferable - token cannot be sold".to_string(),
                    );
                }

                ExtensionType::Pausable | ExtensionType::PausableAccount => {
                    analysis.issues.push(
                        "WARNING: Pausable extension - transfers can be paused".to_string(),
                    );
                }

                ExtensionType::TransferHook => {
                    if let Some(ref hook_program) = ext.transfer_hook_program {
                        match get_hook_safety(hook_program) {
                            HookSafety::Safe => {
                                analysis.issues.push(format!(
                                    "TransferHook (whitelisted: {})",
                                    hook_program
                                ));
                            }
                            HookSafety::Unknown => {
                                analysis.has_transfer_hooks = true;
                                analysis.issues.push(format!(
                                    "WARNING: TransferHook with UNKNOWN program {} - may block sells",
                                    hook_program
                                ));
                            }
                            HookSafety::Malicious => {
                                analysis.has_transfer_hooks = true;
                                analysis.can_transfer = false;
                                analysis.issues.push(format!(
                                    "CRITICAL: TransferHook with MALICIOUS program {}",
                                    hook_program
                                ));
                            }
                        }
                    } else {
                        // TransferHook present but no program set - might be configured later
                        analysis.issues.push(
                            "TransferHook extension present (no program configured)".to_string(),
                        );
                    }
                }

                ExtensionType::TransferFeeConfig => {
                    if let Some(fee_bps) = ext.transfer_fee_bps {
                        analysis.transfer_fee_bps = Some(fee_bps);

                        if fee_bps > 2500 {
                            // >25%
                            analysis.issues.push(format!(
                                "CRITICAL: Transfer fee {}% - effectively untradeable",
                                fee_bps as f64 / 100.0
                            ));
                            analysis.can_transfer = false;
                        } else if fee_bps > 1000 {
                            // >10%
                            analysis.issues.push(format!(
                                "WARNING: High transfer fee {}%",
                                fee_bps as f64 / 100.0
                            ));
                        } else if fee_bps > 0 {
                            analysis.issues.push(format!(
                                "Transfer fee {}%",
                                fee_bps as f64 / 100.0
                            ));
                        }
                    }
                }

                ExtensionType::DefaultAccountState => {
                    if let Some(state) = ext.default_account_state {
                        if state == 2 {
                            // Frozen
                            analysis.issues.push(
                                "WARNING: DefaultAccountState is Frozen - new accounts frozen by default".to_string(),
                            );
                        }
                    }
                }

                ExtensionType::ConfidentialTransferMint => {
                    analysis.issues.push(
                        "ConfidentialTransferMint - transfers may be confidential".to_string(),
                    );
                }

                // Safe extensions that don't affect tradability
                ExtensionType::MetadataPointer
                | ExtensionType::TokenMetadata
                | ExtensionType::ImmutableOwner
                | ExtensionType::MintCloseAuthority => {
                    // These are generally safe, don't add to issues
                }

                _ => {
                    // Log unknown extensions for review
                    if ext.extension_type.needs_review() {
                        analysis.issues.push(format!(
                            "Extension: {} (needs review)",
                            ext.extension_type.name()
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl HoneypotDetector for SolanaHoneypotDetector {
    /// Quick check if token can likely be sold
    async fn can_sell(&self, mint: &Pubkey) -> Result<bool, HoneypotError> {
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(mint) {
                debug!("Cache hit for {}: can_sell={}", mint, cached.can_transfer);
                return Ok(cached.can_transfer && !cached.risk_level.should_block());
            }
        }

        // Fetch mint account
        let (owner, mint_data) = self.fetch_mint_account(mint).await?;

        // Validate token program
        let program = validate_token_program(&owner).map_err(|e| {
            warn!("Unknown token program for {}: {}", mint, e);
            HoneypotError::TransferBlocked {
                reason: format!("Unknown token program: {} - could have arbitrary transfer logic", owner),
            }
        })?;

        // For standard SPL tokens, check authorities only
        if program == TokenProgram::Spl {
            let authorities = parse_authorities(&mint_data).map_err(|e| {
                HoneypotError::RpcError(format!("Failed to parse authorities: {}", e))
            })?;

            // SPL tokens with freeze authority are risky but not blocked
            if authorities.freeze_authority.is_some() && self.config.base.reject_freeze_authority {
                return Ok(false);
            }

            return Ok(true);
        }

        // For Token-2022, check extensions
        let extensions = parse_token2022_extensions(&mint_data).map_err(|e| {
            HoneypotError::RpcError(format!("Failed to parse extensions: {}", e))
        })?;

        // Check for blocking extensions
        for ext in &extensions {
            match ext.extension_type {
                ExtensionType::PermanentDelegate => {
                    warn!("PermanentDelegate detected on {} - blocking", mint);
                    return Ok(false);
                }
                ExtensionType::NonTransferable | ExtensionType::NonTransferableAccount => {
                    warn!("NonTransferable detected on {} - blocking", mint);
                    return Ok(false);
                }
                ExtensionType::TransferHook => {
                    if let Some(ref hook) = ext.transfer_hook_program {
                        if get_hook_safety(hook) != HookSafety::Safe
                            && self.config.base.reject_transfer_hooks
                        {
                            warn!(
                                "Unknown TransferHook {} on {} - blocking",
                                hook, mint
                            );
                            return Ok(false);
                        }
                    }
                }
                ExtensionType::TransferFeeConfig => {
                    if let Some(fee_bps) = ext.transfer_fee_bps {
                        if fee_bps > self.config.base.max_transfer_fee_bps {
                            warn!(
                                "Transfer fee {}bps exceeds max {}bps on {} - blocking",
                                fee_bps, self.config.base.max_transfer_fee_bps, mint
                            );
                            return Ok(false);
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(true)
    }

    /// Get detailed honeypot analysis
    async fn analyze(&self, mint: &Pubkey) -> Result<HoneypotAnalysis, HoneypotError> {
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().await;
            if let Some(cached) = cache.get(mint) {
                info!("Cache hit for {} analysis: {:?}", mint, cached.risk_level);
                return Ok(cached.clone());
            }
        }

        let mut analysis = HoneypotAnalysis::default();

        // Fetch mint account
        let (owner, mint_data) = match self.fetch_mint_account(mint).await {
            Ok(data) => data,
            Err(HoneypotError::TokenNotFound(_)) => {
                return Ok(HoneypotAnalysis::honeypot("Token mint not found"));
            }
            Err(e) => {
                // RPC errors - log warning but allow trade (graceful degradation)
                warn!("RPC error checking {}: {} - allowing trade", mint, e);
                return Ok(HoneypotAnalysis::safe().with_issue(format!(
                    "RPC unavailable - analysis incomplete: {}",
                    e
                )));
            }
        };

        // Validate token program
        match validate_token_program(&owner) {
            Ok(TokenProgram::Spl) => {
                // Standard SPL Token - check authorities only
                analysis.issues.push("SPL Token (standard program)".to_string());
            }
            Ok(TokenProgram::Token2022) => {
                // Token-2022 - check extensions
                analysis.issues.push("Token-2022 program".to_string());
                self.analyze_extensions(&mint_data, &mut analysis)?;
            }
            Err(_) => {
                // Unknown program - CRITICAL
                analysis.can_transfer = false;
                analysis.risk_level = HoneypotRisk::Confirmed;
                analysis.issues.push(format!(
                    "CRITICAL: Unknown token program {} - arbitrary transfer logic possible",
                    owner
                ));
                // Cache and return immediately
                if self.config.enable_cache {
                    let mut cache = self.cache.write().await;
                    cache.insert(*mint, analysis.clone());
                }
                return Ok(analysis);
            }
        }

        // Parse authorities (works for both SPL and Token-2022)
        match parse_authorities(&mint_data) {
            Ok(authorities) => {
                if let Some(ref auth) = authorities.mint_authority {
                    analysis.has_mint_authority = true;
                    if self.config.base.reject_mint_authority {
                        analysis.issues.push(format!(
                            "WARNING: Mint authority active ({}) - can inflate supply",
                            auth
                        ));
                    }
                }

                if let Some(ref auth) = authorities.freeze_authority {
                    analysis.has_freeze_authority = true;
                    if self.config.base.reject_freeze_authority {
                        analysis.issues.push(format!(
                            "WARNING: Freeze authority active ({}) - can freeze accounts",
                            auth
                        ));
                    }
                }
            }
            Err(e) => {
                analysis.issues.push(format!("Could not parse authorities: {}", e));
            }
        }

        // Simulate sell if configured
        if self.config.base.require_sell_simulation {
            match self
                .sell_simulator
                .simulate_sell(mint, self.config.default_simulation_amount)
                .await
            {
                Ok(result) => {
                    if !result.success {
                        analysis.issues.push(format!(
                            "Sell simulation failed: {}",
                            result.error.as_deref().unwrap_or("unknown")
                        ));
                    }
                    analysis.simulation_result = Some(result);
                }
                Err(e) => {
                    // Simulation errors are not blocking (Jupiter API issues)
                    analysis.issues.push(format!("Sell simulation error: {}", e));
                }
            }
        }

        // Calculate risk level
        analysis.calculate_risk();

        info!(
            "Honeypot analysis for {}: {:?} - {} issues",
            mint,
            analysis.risk_level,
            analysis.issues.len()
        );

        // Cache result
        if self.config.enable_cache {
            let mut cache = self.cache.write().await;
            cache.insert(*mint, analysis.clone());
        }

        Ok(analysis)
    }

    /// Simulate a sell transaction
    async fn simulate_sell(
        &self,
        mint: &Pubkey,
        amount: u64,
    ) -> Result<SimulationResult, HoneypotError> {
        self.sell_simulator.simulate_sell(mint, amount).await
    }

    /// Check if token has dangerous Token-2022 extensions
    async fn check_extensions(&self, mint: &Pubkey) -> Result<Vec<String>, HoneypotError> {
        let (owner, mint_data) = self.fetch_mint_account(mint).await?;

        // Validate program
        let program = validate_token_program(&owner).map_err(|e| {
            HoneypotError::TransferBlocked {
                reason: format!("Unknown token program: {}", e),
            }
        })?;

        if program == TokenProgram::Spl {
            // Standard SPL Token has no extensions
            return Ok(vec!["SPL Token (no extensions)".to_string()]);
        }

        // Parse Token-2022 extensions
        let extensions = parse_token2022_extensions(&mint_data).map_err(|e| {
            HoneypotError::RpcError(format!("Failed to parse extensions: {}", e))
        })?;

        let mut result: Vec<String> = extensions
            .iter()
            .map(|ext| {
                let mut desc = ext.extension_type.name().to_string();

                if ext.extension_type.is_dangerous() {
                    desc = format!("DANGEROUS: {}", desc);
                } else if ext.extension_type.needs_review() {
                    desc = format!("REVIEW: {}", desc);
                }

                // Add details for specific extensions
                if let Some(ref hook) = ext.transfer_hook_program {
                    desc = format!("{} (program: {})", desc, hook);
                }
                if let Some(fee_bps) = ext.transfer_fee_bps {
                    desc = format!("{} (fee: {}bps)", desc, fee_bps);
                }
                if let Some(ref delegate) = ext.permanent_delegate {
                    desc = format!("{} (delegate: {})", desc, delegate);
                }

                desc
            })
            .collect();

        if result.is_empty() {
            result.push("Token-2022 (no extensions)".to_string());
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = SolanaHoneypotDetectorConfig::default();
        assert!(config.enable_cache);
        assert_eq!(config.max_price_impact_pct, 25.0);
        assert!(config.base.reject_transfer_hooks);
    }

    #[test]
    fn test_config_strict() {
        let config = SolanaHoneypotDetectorConfig::strict();
        assert_eq!(config.max_price_impact_pct, 15.0);
        assert!(config.base.reject_permanent_delegate);
    }

    // Integration tests would require network access
    // and are better suited for the integration test suite
}
