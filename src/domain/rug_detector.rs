//! Rug Detector
//!
//! Comprehensive rug pull detection system for meme coin trading.
//! Analyzes token metadata, holder distribution, liquidity, and contract
//! patterns to assess the safety risk of trading a token.
//!
//! Common rug patterns detected:
//! 1. **Honeypot**: Can buy but can't sell (transfer restrictions)
//! 2. **LP Pull**: Creator removes liquidity suddenly
//! 3. **Mint Abuse**: Creator can mint unlimited tokens
//! 4. **Freeze**: Creator can freeze holder accounts
//! 5. **Tax Tokens**: High hidden fees on transfer
//!
//! This detector complements the `HoneypotDetector` by providing
//! broader risk assessment beyond just transfer restrictions.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Default configuration values
pub const DEFAULT_MAX_SINGLE_HOLDER_PCT: f64 = 2.0;
pub const DEFAULT_MAX_TOP10_HOLDER_PCT: f64 = 50.0;
pub const DEFAULT_MIN_HOLDER_COUNT: u32 = 100;
pub const DEFAULT_MIN_LIQUIDITY_USD: f64 = 10_000.0;
pub const DEFAULT_TOKEN_AGE_WARNING_HOURS: u64 = 24;
pub const DEFAULT_CACHE_TTL_SECONDS: u64 = 300; // 5 minutes

/// Errors that can occur during rug detection
#[derive(Error, Debug, Clone)]
pub enum RugDetectorError {
    #[error("Failed to fetch token metadata: {0}")]
    MetadataFetchError(String),

    #[error("Failed to fetch holder data: {0}")]
    HolderDataError(String),

    #[error("Failed to fetch liquidity data: {0}")]
    LiquidityDataError(String),

    #[error("Token not found: {mint}")]
    TokenNotFound { mint: String },

    #[error("Analysis timed out after {timeout_secs} seconds")]
    AnalysisTimeout { timeout_secs: u64 },

    #[error("Insufficient data for analysis: {reason}")]
    InsufficientData { reason: String },

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("Cache error: {0}")]
    CacheError(String),

    #[error("Partial data: missing {field}")]
    PartialData { field: String },

    #[error("Invalid data: {field} - {reason}")]
    InvalidData { field: String, reason: String },

    #[error("Division by zero in: {context}")]
    DivisionByZero { context: String },

    #[error("Analysis degraded: {reason}")]
    AnalysisDegraded { reason: String },
}

/// Risk level classification for rug detection
///
/// Risk levels are cumulative - a token flagged as High risk
/// will have issues from Medium and Low levels as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RiskLevel {
    /// Token passed all safety checks
    Safe,
    /// Minor concerns, but generally safe to trade
    Low,
    /// Significant concerns - proceed with caution
    Medium,
    /// High risk of rug pull - avoid or use minimal size
    High,
    /// Critical issues - confirmed rug indicators present
    Critical,
}

impl RiskLevel {
    /// Whether this risk level is acceptable for trading
    pub fn is_tradeable(&self) -> bool {
        matches!(self, RiskLevel::Safe | RiskLevel::Low)
    }

    /// Whether this risk level should trigger a warning
    pub fn should_warn(&self) -> bool {
        matches!(self, RiskLevel::Medium | RiskLevel::High | RiskLevel::Critical)
    }

    /// Whether this risk level should block trading
    pub fn should_block(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Critical)
    }

    /// Get a human-readable description of the risk level
    pub fn description(&self) -> &'static str {
        match self {
            RiskLevel::Safe => "Token passed all safety checks",
            RiskLevel::Low => "Minor concerns detected, generally safe",
            RiskLevel::Medium => "Significant concerns - proceed with caution",
            RiskLevel::High => "High risk of rug pull - avoid or minimize exposure",
            RiskLevel::Critical => "Critical rug indicators - do not trade",
        }
    }

    /// Get recommended position size multiplier (1.0 = full size, 0.0 = no trade)
    pub fn position_size_multiplier(&self) -> f64 {
        match self {
            RiskLevel::Safe => 1.0,
            RiskLevel::Low => 0.75,
            RiskLevel::Medium => 0.25,
            RiskLevel::High => 0.0,
            RiskLevel::Critical => 0.0,
        }
    }
}

impl Default for RiskLevel {
    fn default() -> Self {
        RiskLevel::Safe
    }
}

/// Specific warning types detected during rug analysis
#[derive(Debug, Clone, PartialEq)]
pub enum RugWarning {
    /// Mint authority is still active - creator can mint more tokens
    MintAuthorityActive {
        authority: String,
    },

    /// Freeze authority is still active - creator can freeze accounts
    FreezeAuthorityActive {
        authority: String,
    },

    /// Token creator holds too much supply
    HighCreatorHolding {
        holder: String,
        percentage: f64,
    },

    /// Top holders own too much of the supply (whale concentration)
    ConcentratedHolders {
        top_n: u32,
        combined_percentage: f64,
    },

    /// Liquidity is below safe threshold
    LowLiquidity {
        liquidity_usd: f64,
        minimum_usd: f64,
    },

    /// LP tokens are not burned/locked
    UnlockedLP {
        lp_mint: String,
        holder: String,
    },

    /// Suspicious patterns in token metadata
    SuspiciousMetadata {
        field: String,
        reason: String,
    },

    /// Token was created very recently
    RecentCreation {
        age_hours: u64,
        threshold_hours: u64,
    },

    /// Too few unique holders
    LowHolderCount {
        count: u32,
        minimum: u32,
    },

    /// Suspicious transfer patterns detected
    SuspiciousTransferPattern {
        pattern: String,
    },

    /// Token has known blacklist/honeypot keywords
    BlacklistKeywordDetected {
        keyword: String,
        location: String,
    },

    /// Creator wallet has history of rugs
    CreatorFlagged {
        creator: String,
        reason: String,
    },

    /// High tax/fee detected on transfers
    HighTransferTax {
        tax_bps: u16,
    },

    /// Token-2022 extensions that could be dangerous
    DangerousExtension {
        extension: String,
    },

    /// Liquidity dropped significantly since launch
    LiquidityDrop {
        drop_percentage: f64,
        timeframe_hours: u64,
    },

    /// Multiple related wallets detected (sybil pattern)
    SybilPattern {
        wallet_count: u32,
        combined_percentage: f64,
    },
}

impl RugWarning {
    /// Get the severity score for this warning (0-100)
    pub fn severity_score(&self) -> u32 {
        match self {
            RugWarning::MintAuthorityActive { .. } => 80,
            RugWarning::FreezeAuthorityActive { .. } => 60,
            RugWarning::HighCreatorHolding { percentage, .. } => {
                if *percentage > 50.0 {
                    90
                } else if *percentage > 20.0 {
                    70
                } else if *percentage > 10.0 {
                    50
                } else {
                    30
                }
            }
            RugWarning::ConcentratedHolders { combined_percentage, .. } => {
                if *combined_percentage > 80.0 {
                    85
                } else if *combined_percentage > 60.0 {
                    65
                } else {
                    45
                }
            }
            RugWarning::LowLiquidity { liquidity_usd, minimum_usd } => {
                // Protect against division by zero
                if *minimum_usd <= 0.0 {
                    return 95; // Assume worst case if minimum is invalid
                }
                let ratio = liquidity_usd / minimum_usd;
                if ratio < 0.1 {
                    95
                } else if ratio < 0.5 {
                    70
                } else {
                    40
                }
            }
            RugWarning::UnlockedLP { .. } => 75,
            RugWarning::SuspiciousMetadata { .. } => 35,
            RugWarning::RecentCreation { age_hours, .. } => {
                if *age_hours < 1 {
                    60
                } else if *age_hours < 6 {
                    40
                } else {
                    20
                }
            }
            RugWarning::LowHolderCount { count, minimum } => {
                // Protect against division by zero
                if *minimum == 0 {
                    return 70; // Assume worst case if minimum is invalid
                }
                let ratio = *count as f64 / *minimum as f64;
                if ratio < 0.1 {
                    70
                } else if ratio < 0.5 {
                    50
                } else {
                    30
                }
            }
            RugWarning::SuspiciousTransferPattern { .. } => 55,
            RugWarning::BlacklistKeywordDetected { .. } => 85,
            RugWarning::CreatorFlagged { .. } => 95,
            RugWarning::HighTransferTax { tax_bps } => {
                if *tax_bps > 2000 {
                    // > 20%
                    90
                } else if *tax_bps > 1000 {
                    // > 10%
                    70
                } else if *tax_bps > 500 {
                    // > 5%
                    50
                } else {
                    30
                }
            }
            RugWarning::DangerousExtension { .. } => 80,
            RugWarning::LiquidityDrop { drop_percentage, .. } => {
                if *drop_percentage > 80.0 {
                    90
                } else if *drop_percentage > 50.0 {
                    70
                } else {
                    45
                }
            }
            RugWarning::SybilPattern { combined_percentage, .. } => {
                if *combined_percentage > 50.0 {
                    75
                } else if *combined_percentage > 30.0 {
                    55
                } else {
                    35
                }
            }
        }
    }

    /// Get a short description of the warning
    pub fn short_description(&self) -> String {
        match self {
            RugWarning::MintAuthorityActive { .. } => "Mint authority active".to_string(),
            RugWarning::FreezeAuthorityActive { .. } => "Freeze authority active".to_string(),
            RugWarning::HighCreatorHolding { percentage, .. } => {
                format!("Creator holds {:.1}%", percentage)
            }
            RugWarning::ConcentratedHolders {
                top_n,
                combined_percentage,
            } => format!("Top {} hold {:.1}%", top_n, combined_percentage),
            RugWarning::LowLiquidity { liquidity_usd, .. } => {
                format!("Low liquidity: ${:.0}", liquidity_usd)
            }
            RugWarning::UnlockedLP { .. } => "LP tokens not burned".to_string(),
            RugWarning::SuspiciousMetadata { field, .. } => {
                format!("Suspicious {}", field)
            }
            RugWarning::RecentCreation { age_hours, .. } => {
                format!("Created {} hours ago", age_hours)
            }
            RugWarning::LowHolderCount { count, .. } => format!("Only {} holders", count),
            RugWarning::SuspiciousTransferPattern { pattern } => {
                format!("Suspicious pattern: {}", pattern)
            }
            RugWarning::BlacklistKeywordDetected { keyword, .. } => {
                format!("Blacklisted keyword: {}", keyword)
            }
            RugWarning::CreatorFlagged { .. } => "Creator flagged".to_string(),
            RugWarning::HighTransferTax { tax_bps } => {
                format!("{}% transfer tax", *tax_bps as f64 / 100.0)
            }
            RugWarning::DangerousExtension { extension } => {
                format!("Dangerous extension: {}", extension)
            }
            RugWarning::LiquidityDrop { drop_percentage, .. } => {
                format!("Liquidity dropped {:.1}%", drop_percentage)
            }
            RugWarning::SybilPattern { wallet_count, .. } => {
                format!("{} sybil wallets detected", wallet_count)
            }
        }
    }
}

/// Information about a token holder
#[derive(Debug, Clone)]
pub struct HolderInfo {
    /// Wallet address
    pub address: String,
    /// Amount held (in base units)
    pub amount: u64,
    /// Percentage of total supply
    pub percentage: f64,
    /// Whether this is likely the creator
    pub is_creator: bool,
    /// Whether this is a known exchange/DEX
    pub is_known_exchange: bool,
}

/// Liquidity pool information
#[derive(Debug, Clone)]
pub struct LiquidityInfo {
    /// Pool address
    pub pool_address: String,
    /// LP mint address
    pub lp_mint: String,
    /// Total liquidity in USD
    pub liquidity_usd: f64,
    /// Token amount in pool
    pub token_amount: u64,
    /// SOL/USDC amount in pool
    pub quote_amount: u64,
    /// Whether LP tokens are burned
    pub lp_burned: bool,
    /// LP token holder if not burned
    pub lp_holder: Option<String>,
    /// Percentage of LP locked/burned
    pub lp_locked_percentage: f64,
}

/// Complete safety report for a token
#[derive(Debug, Clone)]
pub struct TokenSafetyReport {
    /// Token mint address
    pub mint: String,
    /// Overall risk level
    pub risk_level: RiskLevel,
    /// List of warnings detected
    pub warnings: Vec<RugWarning>,
    /// List of checks that passed
    pub passed_checks: Vec<String>,
    /// List of checks that failed
    pub failed_checks: Vec<String>,
    /// Timestamp when analysis was performed
    pub analyzed_at: Instant,
    /// Token age in hours (if known)
    pub token_age_hours: Option<u64>,
    /// Total holder count
    pub holder_count: Option<u32>,
    /// Total liquidity in USD
    pub liquidity_usd: Option<f64>,
    /// Creator wallet address (if identified)
    pub creator: Option<String>,
    /// Cumulative risk score (0-100)
    pub risk_score: u32,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

impl TokenSafetyReport {
    /// Create a new safety report
    pub fn new(mint: String) -> Self {
        Self {
            mint,
            risk_level: RiskLevel::Safe,
            warnings: Vec::new(),
            passed_checks: Vec::new(),
            failed_checks: Vec::new(),
            analyzed_at: Instant::now(),
            token_age_hours: None,
            holder_count: None,
            liquidity_usd: None,
            creator: None,
            risk_score: 0,
            metadata: HashMap::new(),
        }
    }

    /// Add a warning to the report
    pub fn add_warning(&mut self, warning: RugWarning) {
        self.warnings.push(warning);
    }

    /// Add a passed check
    pub fn add_passed(&mut self, check: impl Into<String>) {
        self.passed_checks.push(check.into());
    }

    /// Add a failed check
    pub fn add_failed(&mut self, check: impl Into<String>) {
        self.failed_checks.push(check.into());
    }

    /// Calculate the risk score and level based on warnings
    pub fn calculate_risk(&mut self) {
        // Sum up severity scores from all warnings
        let total_score: u32 = self.warnings.iter().map(|w| w.severity_score()).sum();

        // Normalize to 0-100 (cap at 100)
        self.risk_score = total_score.min(100);

        // Determine risk level based on score
        self.risk_level = match self.risk_score {
            0..=15 => RiskLevel::Safe,
            16..=35 => RiskLevel::Low,
            36..=55 => RiskLevel::Medium,
            56..=80 => RiskLevel::High,
            _ => RiskLevel::Critical,
        };
    }

    /// Check if the token is safe to trade
    pub fn is_safe(&self) -> bool {
        self.risk_level.is_tradeable()
    }

    /// Get a summary of the report
    pub fn summary(&self) -> String {
        format!(
            "Token {} - Risk: {:?} (score: {}), {} warnings, {} passed, {} failed",
            self.mint,
            self.risk_level,
            self.risk_score,
            self.warnings.len(),
            self.passed_checks.len(),
            self.failed_checks.len()
        )
    }

    /// Get time since analysis
    pub fn age(&self) -> Duration {
        self.analyzed_at.elapsed()
    }

    /// Check if the report is still fresh
    pub fn is_fresh(&self, max_age: Duration) -> bool {
        self.age() < max_age
    }
}

impl Default for TokenSafetyReport {
    fn default() -> Self {
        Self::new(String::new())
    }
}

/// Configuration for the rug detector
#[derive(Debug, Clone)]
pub struct RugDetectorConfig {
    // Authority checks
    /// Require mint authority to be revoked (null)
    pub require_revoked_mint: bool,
    /// Require freeze authority to be revoked (null)
    pub require_revoked_freeze: bool,

    // Holder distribution
    /// Maximum percentage any single holder can own
    pub max_single_holder_percent: f64,
    /// Maximum percentage top 10 holders can own combined
    pub max_top10_holder_percent: f64,
    /// Minimum number of unique holders
    pub min_holder_count: u32,

    // Liquidity checks
    /// Minimum liquidity in USD
    pub min_liquidity_usd: f64,
    /// Require LP tokens to be burned/locked
    pub require_burned_lp: bool,
    /// Minimum percentage of LP that must be locked
    pub min_lp_locked_percent: f64,

    // Contract checks
    /// Keywords that indicate potential honeypot/scam
    pub blacklist_keywords: Vec<String>,
    /// Dangerous Token-2022 extensions to flag
    pub dangerous_extensions: Vec<String>,

    // Age and timing
    /// Minimum token age in hours (warn if younger)
    pub min_token_age_hours: u64,

    // Transfer restrictions
    /// Maximum acceptable transfer tax (basis points)
    pub max_transfer_tax_bps: u16,

    // Cache settings
    /// How long to cache reports (seconds)
    pub cache_ttl_seconds: u64,

    // Analysis settings
    /// Timeout for analysis operations (seconds)
    pub analysis_timeout_seconds: u64,
}

impl Default for RugDetectorConfig {
    fn default() -> Self {
        Self {
            require_revoked_mint: true,
            require_revoked_freeze: true,
            max_single_holder_percent: DEFAULT_MAX_SINGLE_HOLDER_PCT,
            max_top10_holder_percent: DEFAULT_MAX_TOP10_HOLDER_PCT,
            min_holder_count: DEFAULT_MIN_HOLDER_COUNT,
            min_liquidity_usd: DEFAULT_MIN_LIQUIDITY_USD,
            require_burned_lp: true,
            min_lp_locked_percent: 95.0,
            blacklist_keywords: Self::default_blacklist_keywords(),
            dangerous_extensions: Self::default_dangerous_extensions(),
            min_token_age_hours: DEFAULT_TOKEN_AGE_WARNING_HOURS,
            max_transfer_tax_bps: 500, // 5%
            cache_ttl_seconds: DEFAULT_CACHE_TTL_SECONDS,
            analysis_timeout_seconds: 30,
        }
    }
}

impl RugDetectorConfig {
    /// Default blacklist keywords for honeypot detection
    fn default_blacklist_keywords() -> Vec<String> {
        vec![
            "honeypot".to_string(),
            "honey_pot".to_string(),
            "hp_".to_string(),
            "_hp".to_string(),
            "cantSell".to_string(),
            "cant_sell".to_string(),
            "nosell".to_string(),
            "no_sell".to_string(),
            "blacklist".to_string(),
            "black_list".to_string(),
            "whitelist_only".to_string(),
            "pausable".to_string(),
            "pause_trading".to_string(),
            "max_tx".to_string(),
            "max_wallet".to_string(),
            "anti_bot".to_string(),
            "antibot".to_string(),
            "cooldown".to_string(),
            "fee_on_transfer".to_string(),
            "tax_".to_string(),
            "_tax".to_string(),
            "burn_fee".to_string(),
            "reflect".to_string(),
            "rebase".to_string(),
        ]
    }

    /// Default dangerous Token-2022 extensions
    fn default_dangerous_extensions() -> Vec<String> {
        vec![
            "TransferHook".to_string(),
            "PermanentDelegate".to_string(),
            "TransferFeeConfig".to_string(),
            "ConfidentialTransferMint".to_string(),
            "NonTransferable".to_string(),
            "InterestBearingConfig".to_string(),
        ]
    }

    /// Create a strict configuration (most conservative)
    pub fn strict() -> Self {
        Self {
            require_revoked_mint: true,
            require_revoked_freeze: true,
            max_single_holder_percent: 1.0,
            max_top10_holder_percent: 30.0,
            min_holder_count: 500,
            min_liquidity_usd: 50_000.0,
            require_burned_lp: true,
            min_lp_locked_percent: 99.0,
            min_token_age_hours: 48,
            max_transfer_tax_bps: 100, // 1%
            ..Default::default()
        }
    }

    /// Create a lenient configuration (more permissive for early tokens)
    pub fn lenient() -> Self {
        Self {
            require_revoked_mint: true, // Never allow active mint
            require_revoked_freeze: false,
            max_single_holder_percent: 5.0,
            max_top10_holder_percent: 70.0,
            min_holder_count: 50,
            min_liquidity_usd: 5_000.0,
            require_burned_lp: false,
            min_lp_locked_percent: 50.0,
            min_token_age_hours: 1,
            max_transfer_tax_bps: 1000, // 10%
            ..Default::default()
        }
    }

    /// Create configuration for pump.fun sniping (very early tokens)
    pub fn pump_fun_sniper() -> Self {
        Self {
            require_revoked_mint: true,
            require_revoked_freeze: false,
            max_single_holder_percent: 10.0,
            max_top10_holder_percent: 90.0, // Early tokens are concentrated
            min_holder_count: 10,
            min_liquidity_usd: 1_000.0,
            require_burned_lp: false,
            min_lp_locked_percent: 0.0,
            min_token_age_hours: 0, // Brand new is fine
            max_transfer_tax_bps: 500,
            ..Default::default()
        }
    }
}

/// Token metadata for rug detection
#[derive(Debug, Clone)]
pub struct TokenAnalysisData {
    /// Token mint address
    pub mint: String,
    /// Mint authority (None = revoked)
    pub mint_authority: Option<String>,
    /// Freeze authority (None = revoked)
    pub freeze_authority: Option<String>,
    /// Total supply
    pub supply: u64,
    /// Decimals
    pub decimals: u8,
    /// Token name
    pub name: Option<String>,
    /// Token symbol
    pub symbol: Option<String>,
    /// Token URI (metadata)
    pub uri: Option<String>,
    /// Creation timestamp (unix seconds)
    pub created_at: Option<u64>,
    /// Token-2022 extensions present
    pub extensions: Vec<String>,
    /// Transfer fee in basis points
    pub transfer_fee_bps: Option<u16>,
}

/// Main rug detector struct
pub struct RugDetector {
    /// Configuration
    config: RugDetectorConfig,
    /// Cache of recent reports
    token_cache: HashMap<String, TokenSafetyReport>,
    /// Known flagged creators
    flagged_creators: HashMap<String, String>,
    /// Known exchange/DEX addresses
    known_exchanges: Vec<String>,
}

impl RugDetector {
    /// Create a new rug detector with default configuration
    pub fn new() -> Self {
        Self::with_config(RugDetectorConfig::default())
    }

    /// Create a new rug detector with custom configuration
    pub fn with_config(config: RugDetectorConfig) -> Self {
        Self {
            config,
            token_cache: HashMap::new(),
            flagged_creators: HashMap::new(),
            known_exchanges: Self::default_known_exchanges(),
        }
    }

    /// Create a strict rug detector
    pub fn strict() -> Self {
        Self::with_config(RugDetectorConfig::strict())
    }

    /// Create a lenient rug detector
    pub fn lenient() -> Self {
        Self::with_config(RugDetectorConfig::lenient())
    }

    /// Create a pump.fun sniper detector
    pub fn pump_fun_sniper() -> Self {
        Self::with_config(RugDetectorConfig::pump_fun_sniper())
    }

    /// Default known exchange addresses
    fn default_known_exchanges() -> Vec<String> {
        vec![
            // Raydium AMM
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8".to_string(),
            // Orca Whirlpool
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc".to_string(),
            // Jupiter Aggregator
            "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4".to_string(),
            // Meteora
            "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo".to_string(),
        ]
    }

    /// Add a flagged creator
    pub fn flag_creator(&mut self, address: String, reason: String) {
        self.flagged_creators.insert(address, reason);
    }

    /// Add a known exchange address
    pub fn add_known_exchange(&mut self, address: String) {
        if !self.known_exchanges.contains(&address) {
            self.known_exchanges.push(address);
        }
    }

    /// Check if address is a known exchange
    pub fn is_known_exchange(&self, address: &str) -> bool {
        self.known_exchanges.contains(&address.to_string())
    }

    /// Get cached report if fresh
    pub fn get_cached(&self, mint: &str) -> Option<&TokenSafetyReport> {
        self.token_cache.get(mint).filter(|report| {
            report.is_fresh(Duration::from_secs(self.config.cache_ttl_seconds))
        })
    }

    /// Analyze a token with provided data
    ///
    /// This is the main analysis method that takes pre-fetched data.
    /// For async fetching, use `analyze` instead.
    pub fn analyze_with_data(
        &mut self,
        token_data: &TokenAnalysisData,
        holders: &[HolderInfo],
        liquidity: Option<&LiquidityInfo>,
    ) -> TokenSafetyReport {
        let mut report = TokenSafetyReport::new(token_data.mint.clone());

        // Authority checks
        self.check_authorities(token_data, &mut report);

        // Holder distribution checks
        self.check_holder_distribution(holders, &mut report);

        // Liquidity checks
        if let Some(liq) = liquidity {
            self.check_liquidity(liq, &mut report);
        } else {
            report.add_failed("liquidity_data_missing");
        }

        // Contract/metadata checks
        self.check_metadata(token_data, &mut report);

        // Token age check
        self.check_token_age(token_data, &mut report);

        // Transfer fee check
        self.check_transfer_fee(token_data, &mut report);

        // Extensions check
        self.check_extensions(token_data, &mut report);

        // Identify creator and check if flagged
        self.check_creator(token_data, holders, &mut report);

        // Calculate final risk
        report.calculate_risk();

        // Cache the report
        self.token_cache
            .insert(token_data.mint.clone(), report.clone());

        report
    }

    /// Analyze a token with partial data, applying graceful degradation
    ///
    /// This method allows analysis even when some data is missing. It will:
    /// - Skip checks for missing data instead of failing
    /// - Add metadata about which checks were skipped
    /// - Apply a conservative risk bump for missing data
    pub fn analyze_with_partial_data(
        &mut self,
        token_data: Option<&TokenAnalysisData>,
        holders: Option<&[HolderInfo]>,
        liquidity: Option<&LiquidityInfo>,
        mint: &str,
    ) -> Result<TokenSafetyReport, RugDetectorError> {
        // We need at least mint address to create a report
        if mint.is_empty() {
            return Err(RugDetectorError::InsufficientData {
                reason: "mint address is required".to_string(),
            });
        }

        let mut report = TokenSafetyReport::new(mint.to_string());
        let mut skipped_checks = Vec::new();

        // Token data checks
        if let Some(data) = token_data {
            self.check_authorities(data, &mut report);
            self.check_metadata(data, &mut report);
            self.check_token_age(data, &mut report);
            self.check_transfer_fee(data, &mut report);
            self.check_extensions(data, &mut report);
        } else {
            skipped_checks.push("token_metadata");
            report.add_failed("token_data_unavailable");
        }

        // Holder distribution checks
        if let Some(holders_data) = holders {
            if !holders_data.is_empty() {
                self.check_holder_distribution(holders_data, &mut report);

                // Creator check requires both token data and holders
                if let Some(data) = token_data {
                    self.check_creator(data, holders_data, &mut report);
                }
            } else {
                skipped_checks.push("holder_distribution");
                report.add_failed("holder_data_empty");
            }
        } else {
            skipped_checks.push("holder_distribution");
            report.add_failed("holder_data_unavailable");
        }

        // Liquidity checks
        if let Some(liq) = liquidity {
            self.check_liquidity(liq, &mut report);
        } else {
            skipped_checks.push("liquidity");
            report.add_failed("liquidity_data_unavailable");
        }

        // Record skipped checks in metadata
        if !skipped_checks.is_empty() {
            report
                .metadata
                .insert("skipped_checks".to_string(), skipped_checks.join(", "));
            report.metadata.insert("partial_analysis".to_string(), "true".to_string());
        }

        // Calculate risk
        report.calculate_risk();

        // Apply conservative bump for missing data (increase risk level if data is missing)
        if !skipped_checks.is_empty() && report.risk_level < RiskLevel::Medium {
            // Bump to at least Low risk when data is missing
            if report.risk_level == RiskLevel::Safe {
                report.risk_level = RiskLevel::Low;
                report.risk_score = report.risk_score.max(20);
            }
        }

        // Cache the report
        self.token_cache.insert(mint.to_string(), report.clone());

        Ok(report)
    }

    /// Invalidate cache for a specific token on error
    pub fn invalidate_cache(&mut self, mint: &str) {
        self.token_cache.remove(mint);
    }

    /// Invalidate cache entries older than specified duration
    pub fn invalidate_stale_cache(&mut self, max_age: Duration) {
        self.token_cache.retain(|_, report| report.is_fresh(max_age));
    }

    /// Check mint and freeze authorities
    fn check_authorities(&self, data: &TokenAnalysisData, report: &mut TokenSafetyReport) {
        // Mint authority check
        if let Some(ref authority) = data.mint_authority {
            if self.config.require_revoked_mint {
                report.add_warning(RugWarning::MintAuthorityActive {
                    authority: authority.clone(),
                });
                report.add_failed("mint_authority_revoked");
            } else {
                report.add_passed("mint_authority_check_skipped");
            }
        } else {
            report.add_passed("mint_authority_revoked");
        }

        // Freeze authority check
        if let Some(ref authority) = data.freeze_authority {
            if self.config.require_revoked_freeze {
                report.add_warning(RugWarning::FreezeAuthorityActive {
                    authority: authority.clone(),
                });
                report.add_failed("freeze_authority_revoked");
            } else {
                report.add_passed("freeze_authority_check_skipped");
            }
        } else {
            report.add_passed("freeze_authority_revoked");
        }
    }

    /// Check holder distribution
    fn check_holder_distribution(&self, holders: &[HolderInfo], report: &mut TokenSafetyReport) {
        report.holder_count = Some(holders.len() as u32);

        // Check minimum holder count
        if (holders.len() as u32) < self.config.min_holder_count {
            report.add_warning(RugWarning::LowHolderCount {
                count: holders.len() as u32,
                minimum: self.config.min_holder_count,
            });
            report.add_failed("min_holder_count");
        } else {
            report.add_passed("min_holder_count");
        }

        // Filter out known exchanges for concentration checks
        let non_exchange_holders: Vec<_> = holders
            .iter()
            .filter(|h| !h.is_known_exchange && !self.is_known_exchange(&h.address))
            .collect();

        // Check single holder concentration
        for holder in &non_exchange_holders {
            if holder.percentage > self.config.max_single_holder_percent {
                report.add_warning(RugWarning::HighCreatorHolding {
                    holder: holder.address.clone(),
                    percentage: holder.percentage,
                });
            }
        }

        // Check top 10 concentration
        let mut sorted_holders = non_exchange_holders.clone();
        sorted_holders.sort_by(|a, b| {
            b.percentage
                .partial_cmp(&a.percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top10: Vec<_> = sorted_holders.into_iter().take(10).collect();
        let top10_total: f64 = top10.iter().map(|h| h.percentage).sum();

        if top10_total > self.config.max_top10_holder_percent {
            report.add_warning(RugWarning::ConcentratedHolders {
                top_n: top10.len() as u32,
                combined_percentage: top10_total,
            });
            report.add_failed("top10_concentration");
        } else {
            report.add_passed("top10_concentration");
        }

        // Check for sybil patterns (many wallets with similar amounts)
        self.check_sybil_pattern(holders, report);
    }

    /// Check for sybil attack patterns
    fn check_sybil_pattern(&self, holders: &[HolderInfo], report: &mut TokenSafetyReport) {
        // Group holders by similar percentage (within 0.1%)
        let mut amount_groups: HashMap<u64, Vec<&HolderInfo>> = HashMap::new();
        for holder in holders {
            // Round to nearest 0.1%
            let bucket = (holder.percentage * 10.0).round() as u64;
            amount_groups.entry(bucket).or_default().push(holder);
        }

        // Find suspicious groups (5+ wallets with same amount)
        for (_bucket, group) in amount_groups.iter() {
            if group.len() >= 5 {
                let combined: f64 = group.iter().map(|h| h.percentage).sum();
                if combined > 10.0 {
                    report.add_warning(RugWarning::SybilPattern {
                        wallet_count: group.len() as u32,
                        combined_percentage: combined,
                    });
                    break;
                }
            }
        }
    }

    /// Check liquidity
    fn check_liquidity(&self, liquidity: &LiquidityInfo, report: &mut TokenSafetyReport) {
        report.liquidity_usd = Some(liquidity.liquidity_usd);

        // Check minimum liquidity
        if liquidity.liquidity_usd < self.config.min_liquidity_usd {
            report.add_warning(RugWarning::LowLiquidity {
                liquidity_usd: liquidity.liquidity_usd,
                minimum_usd: self.config.min_liquidity_usd,
            });
            report.add_failed("min_liquidity");
        } else {
            report.add_passed("min_liquidity");
        }

        // Check LP burned/locked
        if self.config.require_burned_lp && !liquidity.lp_burned {
            if liquidity.lp_locked_percentage < self.config.min_lp_locked_percent {
                report.add_warning(RugWarning::UnlockedLP {
                    lp_mint: liquidity.lp_mint.clone(),
                    holder: liquidity.lp_holder.clone().unwrap_or_default(),
                });
                report.add_failed("lp_burned_or_locked");
            } else {
                report.add_passed("lp_locked");
            }
        } else if liquidity.lp_burned {
            report.add_passed("lp_burned");
        } else {
            report.add_passed("lp_check_skipped");
        }
    }

    /// Check token metadata for suspicious patterns
    fn check_metadata(&self, data: &TokenAnalysisData, report: &mut TokenSafetyReport) {
        let check_text = |text: &str, field: &str| -> Option<RugWarning> {
            let lower = text.to_lowercase();
            for keyword in &self.config.blacklist_keywords {
                if lower.contains(&keyword.to_lowercase()) {
                    return Some(RugWarning::BlacklistKeywordDetected {
                        keyword: keyword.clone(),
                        location: field.to_string(),
                    });
                }
            }
            None
        };

        // Check name
        if let Some(ref name) = data.name {
            if let Some(warning) = check_text(name, "name") {
                report.add_warning(warning);
            }
        }

        // Check symbol
        if let Some(ref symbol) = data.symbol {
            if let Some(warning) = check_text(symbol, "symbol") {
                report.add_warning(warning);
            }
        }

        // Check URI
        if let Some(ref uri) = data.uri {
            if let Some(warning) = check_text(uri, "uri") {
                report.add_warning(warning);
            }
        }
    }

    /// Check token age
    fn check_token_age(&self, data: &TokenAnalysisData, report: &mut TokenSafetyReport) {
        if let Some(created_at) = data.created_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            let age_hours = (now.saturating_sub(created_at)) / 3600;
            report.token_age_hours = Some(age_hours);

            if age_hours < self.config.min_token_age_hours {
                report.add_warning(RugWarning::RecentCreation {
                    age_hours,
                    threshold_hours: self.config.min_token_age_hours,
                });
                report.add_failed("min_token_age");
            } else {
                report.add_passed("min_token_age");
            }
        }
    }

    /// Check transfer fee
    fn check_transfer_fee(&self, data: &TokenAnalysisData, report: &mut TokenSafetyReport) {
        if let Some(fee_bps) = data.transfer_fee_bps {
            if fee_bps > self.config.max_transfer_tax_bps {
                report.add_warning(RugWarning::HighTransferTax { tax_bps: fee_bps });
                report.add_failed("transfer_fee");
            } else if fee_bps > 0 {
                report.add_passed("transfer_fee_acceptable");
            } else {
                report.add_passed("no_transfer_fee");
            }
        } else {
            report.add_passed("no_transfer_fee");
        }
    }

    /// Check Token-2022 extensions
    fn check_extensions(&self, data: &TokenAnalysisData, report: &mut TokenSafetyReport) {
        for ext in &data.extensions {
            if self.config.dangerous_extensions.contains(ext) {
                report.add_warning(RugWarning::DangerousExtension {
                    extension: ext.clone(),
                });
            }
        }

        if report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::DangerousExtension { .. }))
        {
            report.add_failed("safe_extensions");
        } else {
            report.add_passed("safe_extensions");
        }
    }

    /// Check creator and flagged status
    fn check_creator(
        &self,
        data: &TokenAnalysisData,
        holders: &[HolderInfo],
        report: &mut TokenSafetyReport,
    ) {
        // Try to identify creator from mint authority or largest holder
        let creator = data.mint_authority.clone().or_else(|| {
            holders
                .iter()
                .filter(|h| h.is_creator)
                .next()
                .map(|h| h.address.clone())
        });

        if let Some(ref creator_addr) = creator {
            report.creator = Some(creator_addr.clone());

            // Check if creator is flagged
            if let Some(reason) = self.flagged_creators.get(creator_addr) {
                report.add_warning(RugWarning::CreatorFlagged {
                    creator: creator_addr.clone(),
                    reason: reason.clone(),
                });
                report.add_failed("creator_not_flagged");
            } else {
                report.add_passed("creator_not_flagged");
            }
        }
    }

    /// Quick check if a token is safe (from cache or basic checks)
    pub fn is_safe(&self, mint: &str) -> Option<bool> {
        self.get_cached(mint).map(|r| r.is_safe())
    }

    /// Get warnings for a token (from cache)
    pub fn get_warnings(&self, mint: &str) -> Vec<RugWarning> {
        self.get_cached(mint)
            .map(|r| r.warnings.clone())
            .unwrap_or_default()
    }

    /// Get the configuration
    pub fn config(&self) -> &RugDetectorConfig {
        &self.config
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.token_cache.clear();
    }

    /// Remove stale entries from cache
    pub fn cleanup_cache(&mut self) {
        let ttl = Duration::from_secs(self.config.cache_ttl_seconds);
        self.token_cache.retain(|_, report| report.is_fresh(ttl));
    }
}

impl Default for RugDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_token_data() -> TokenAnalysisData {
        TokenAnalysisData {
            mint: "TestMint123456789".to_string(),
            mint_authority: None,
            freeze_authority: None,
            supply: 1_000_000_000_000,
            decimals: 9,
            name: Some("Safe Token".to_string()),
            symbol: Some("SAFE".to_string()),
            uri: None,
            created_at: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    - 100 * 3600,
            ), // 100 hours ago
            extensions: Vec::new(),
            transfer_fee_bps: None,
        }
    }

    fn create_test_holders(count: usize, max_pct: f64) -> Vec<HolderInfo> {
        let mut holders = Vec::new();
        let base_pct = 100.0 / count as f64;

        for i in 0..count {
            holders.push(HolderInfo {
                address: format!("Holder{}", i),
                amount: 1_000_000_000,
                percentage: if i == 0 { max_pct } else { base_pct.min(100.0 - max_pct) / (count - 1) as f64 },
                is_creator: i == 0,
                is_known_exchange: false,
            });
        }
        holders
    }

    fn create_test_liquidity() -> LiquidityInfo {
        LiquidityInfo {
            pool_address: "Pool123".to_string(),
            lp_mint: "LPMint123".to_string(),
            liquidity_usd: 50_000.0,
            token_amount: 500_000_000_000,
            quote_amount: 1_000_000_000,
            lp_burned: true,
            lp_holder: None,
            lp_locked_percentage: 100.0,
        }
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::Safe < RiskLevel::Low);
        assert!(RiskLevel::Low < RiskLevel::Medium);
        assert!(RiskLevel::Medium < RiskLevel::High);
        assert!(RiskLevel::High < RiskLevel::Critical);
    }

    #[test]
    fn test_risk_level_tradeable() {
        assert!(RiskLevel::Safe.is_tradeable());
        assert!(RiskLevel::Low.is_tradeable());
        assert!(!RiskLevel::Medium.is_tradeable());
        assert!(!RiskLevel::High.is_tradeable());
        assert!(!RiskLevel::Critical.is_tradeable());
    }

    #[test]
    fn test_risk_level_should_block() {
        assert!(!RiskLevel::Safe.should_block());
        assert!(!RiskLevel::Low.should_block());
        assert!(!RiskLevel::Medium.should_block());
        assert!(RiskLevel::High.should_block());
        assert!(RiskLevel::Critical.should_block());
    }

    #[test]
    fn test_position_size_multiplier() {
        assert_eq!(RiskLevel::Safe.position_size_multiplier(), 1.0);
        assert_eq!(RiskLevel::Low.position_size_multiplier(), 0.75);
        assert_eq!(RiskLevel::Medium.position_size_multiplier(), 0.25);
        assert_eq!(RiskLevel::High.position_size_multiplier(), 0.0);
        assert_eq!(RiskLevel::Critical.position_size_multiplier(), 0.0);
    }

    #[test]
    fn test_warning_severity_scores() {
        assert!(RugWarning::MintAuthorityActive {
            authority: "auth".to_string()
        }
        .severity_score()
            > 50);

        assert!(RugWarning::HighCreatorHolding {
            holder: "holder".to_string(),
            percentage: 60.0
        }
        .severity_score()
            > 80);

        assert!(RugWarning::LowLiquidity {
            liquidity_usd: 500.0,
            minimum_usd: 10_000.0
        }
        .severity_score()
            > 60);
    }

    #[test]
    fn test_default_config() {
        let config = RugDetectorConfig::default();
        assert!(config.require_revoked_mint);
        assert!(config.require_revoked_freeze);
        assert_eq!(config.max_single_holder_percent, DEFAULT_MAX_SINGLE_HOLDER_PCT);
        assert_eq!(config.min_liquidity_usd, DEFAULT_MIN_LIQUIDITY_USD);
    }

    #[test]
    fn test_strict_config() {
        let config = RugDetectorConfig::strict();
        assert!(config.max_single_holder_percent < DEFAULT_MAX_SINGLE_HOLDER_PCT);
        assert!(config.min_liquidity_usd > DEFAULT_MIN_LIQUIDITY_USD);
        assert!(config.min_holder_count > DEFAULT_MIN_HOLDER_COUNT);
    }

    #[test]
    fn test_lenient_config() {
        let config = RugDetectorConfig::lenient();
        assert!(config.max_single_holder_percent > DEFAULT_MAX_SINGLE_HOLDER_PCT);
        assert!(config.min_liquidity_usd < DEFAULT_MIN_LIQUIDITY_USD);
    }

    #[test]
    fn test_pump_fun_sniper_config() {
        let config = RugDetectorConfig::pump_fun_sniper();
        assert!(config.min_holder_count < 50);
        assert!(config.min_liquidity_usd < 5_000.0);
        assert_eq!(config.min_token_age_hours, 0);
    }

    #[test]
    fn test_analyze_safe_token() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report.is_safe());
        assert!(report.warnings.is_empty() || report.risk_level <= RiskLevel::Low);
        assert!(report.passed_checks.len() > 0);
    }

    #[test]
    fn test_analyze_token_with_active_mint() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.mint_authority = Some("MintAuth123".to_string());

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(!report.is_safe());
        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::MintAuthorityActive { .. })));
        assert!(report.failed_checks.contains(&"mint_authority_revoked".to_string()));
    }

    #[test]
    fn test_analyze_token_with_high_concentration() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(50, 30.0); // 30% in one wallet
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::HighCreatorHolding { .. })));
    }

    #[test]
    fn test_analyze_token_with_low_liquidity() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let mut liquidity = create_test_liquidity();
        liquidity.liquidity_usd = 500.0; // Very low

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::LowLiquidity { .. })));
    }

    #[test]
    fn test_analyze_token_with_unlocked_lp() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let mut liquidity = create_test_liquidity();
        liquidity.lp_burned = false;
        liquidity.lp_locked_percentage = 0.0;
        liquidity.lp_holder = Some("Creator123".to_string());

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::UnlockedLP { .. })));
    }

    #[test]
    fn test_analyze_token_with_dangerous_extension() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.extensions = vec!["TransferHook".to_string()];

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::DangerousExtension { .. })));
    }

    #[test]
    fn test_analyze_token_with_high_tax() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.transfer_fee_bps = Some(2500); // 25%

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::HighTransferTax { .. })));
    }

    #[test]
    fn test_analyze_recent_token() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.created_at = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 3600,
        ); // 1 hour ago

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::RecentCreation { .. })));
    }

    #[test]
    fn test_blacklist_keyword_detection() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.name = Some("HoneypotToken".to_string());

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::BlacklistKeywordDetected { .. })));
    }

    #[test]
    fn test_flagged_creator_detection() {
        let mut detector = RugDetector::new();
        detector.flag_creator("Creator123".to_string(), "Previous rug pull".to_string());

        let mut token_data = create_test_token_data();
        token_data.mint_authority = Some("Creator123".to_string());

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::CreatorFlagged { .. })));
    }

    #[test]
    fn test_cache_functionality() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        // First analysis
        let _ = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        // Check cache
        assert!(detector.get_cached(&token_data.mint).is_some());
        assert!(detector.is_safe(&token_data.mint).is_some());
    }

    #[test]
    fn test_low_holder_count_warning() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(20, 1.5); // Only 20 holders
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::LowHolderCount { .. })));
    }

    #[test]
    fn test_sybil_pattern_detection() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();

        // Create holders with suspicious pattern (many with same percentage)
        let mut holders = Vec::new();
        for i in 0..10 {
            holders.push(HolderInfo {
                address: format!("SybilWallet{}", i),
                amount: 1_000_000_000,
                percentage: 5.0, // All exactly 5%
                is_creator: false,
                is_known_exchange: false,
            });
        }
        // Add some normal holders
        for i in 10..100 {
            holders.push(HolderInfo {
                address: format!("NormalWallet{}", i),
                amount: 100_000_000,
                percentage: 0.5,
                is_creator: false,
                is_known_exchange: false,
            });
        }

        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::SybilPattern { .. })));
    }

    #[test]
    fn test_risk_score_calculation() {
        let mut report = TokenSafetyReport::new("Test".to_string());

        // No warnings = Safe
        report.calculate_risk();
        assert_eq!(report.risk_level, RiskLevel::Safe);
        assert_eq!(report.risk_score, 0);

        // Add a warning
        report.add_warning(RugWarning::MintAuthorityActive {
            authority: "auth".to_string(),
        });
        report.calculate_risk();
        assert!(report.risk_score > 0);
        assert!(report.risk_level > RiskLevel::Safe);
    }

    #[test]
    fn test_report_summary() {
        let mut report = TokenSafetyReport::new("TestMint123".to_string());
        report.add_passed("check1");
        report.add_failed("check2");
        report.add_warning(RugWarning::LowLiquidity {
            liquidity_usd: 1000.0,
            minimum_usd: 10000.0,
        });
        report.calculate_risk();

        let summary = report.summary();
        assert!(summary.contains("TestMint123"));
        assert!(summary.contains("1 warnings"));
        assert!(summary.contains("1 passed"));
        assert!(summary.contains("1 failed"));
    }

    #[test]
    fn test_known_exchange_filtering() {
        let mut detector = RugDetector::new();
        detector.add_known_exchange("Exchange123".to_string());

        assert!(detector.is_known_exchange("Exchange123"));
        assert!(!detector.is_known_exchange("RandomWallet"));
    }

    #[test]
    fn test_freeze_authority_warning() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.freeze_authority = Some("FreezeAuth123".to_string());

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::FreezeAuthorityActive { .. })));
    }

    #[test]
    fn test_lenient_allows_freeze_authority() {
        let mut detector = RugDetector::lenient();
        let mut token_data = create_test_token_data();
        token_data.freeze_authority = Some("FreezeAuth123".to_string());

        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        // Lenient config should not warn about freeze authority
        assert!(!report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::FreezeAuthorityActive { .. })));
    }

    #[test]
    fn test_warning_short_description() {
        let warning = RugWarning::LowLiquidity {
            liquidity_usd: 500.0,
            minimum_usd: 10000.0,
        };
        let desc = warning.short_description();
        assert!(desc.contains("500"));

        let warning2 = RugWarning::HighCreatorHolding {
            holder: "holder".to_string(),
            percentage: 25.5,
        };
        let desc2 = warning2.short_description();
        assert!(desc2.contains("25.5"));
    }

    #[test]
    fn test_report_age() {
        let report = TokenSafetyReport::new("Test".to_string());
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(report.age() >= Duration::from_millis(10));
    }

    #[test]
    fn test_report_freshness() {
        let report = TokenSafetyReport::new("Test".to_string());
        assert!(report.is_fresh(Duration::from_secs(60)));
        assert!(!report.is_fresh(Duration::from_nanos(1)));
    }

    #[test]
    fn test_cache_cleanup() {
        let mut detector = RugDetector::with_config(RugDetectorConfig {
            cache_ttl_seconds: 0, // Immediate expiry for testing
            ..Default::default()
        });

        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let _ = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(detector.token_cache.len() > 0);

        // Wait a bit and cleanup
        std::thread::sleep(std::time::Duration::from_millis(10));
        detector.cleanup_cache();

        // Cache should be empty after cleanup due to 0 TTL
        assert!(detector.token_cache.is_empty());
    }

    #[test]
    fn test_clear_cache() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        let _ = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(!detector.token_cache.is_empty());

        detector.clear_cache();
        assert!(detector.token_cache.is_empty());
    }

    #[test]
    fn test_critical_risk_multiple_warnings() {
        let mut detector = RugDetector::new();
        let mut token_data = create_test_token_data();
        token_data.mint_authority = Some("MintAuth".to_string());
        token_data.freeze_authority = Some("FreezeAuth".to_string());
        token_data.extensions = vec!["TransferHook".to_string()];
        token_data.name = Some("HoneypotScam".to_string());

        let holders = create_test_holders(20, 50.0); // Few holders, high concentration
        let mut liquidity = create_test_liquidity();
        liquidity.liquidity_usd = 100.0; // Very low liquidity
        liquidity.lp_burned = false;
        liquidity.lp_locked_percentage = 0.0;

        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));

        assert_eq!(report.risk_level, RiskLevel::Critical);
        assert!(report.warnings.len() >= 5);
        assert!(!report.is_safe());
    }

    // ===== Edge Case Tests for Error Handling =====

    #[test]
    fn test_partial_data_analysis_token_only() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();

        // Analyze with only token data (no holders, no liquidity)
        let result = detector.analyze_with_partial_data(
            Some(&token_data),
            None,
            None,
            &token_data.mint,
        );

        assert!(result.is_ok());
        let report = result.unwrap();

        // Should be bumped to at least Low risk due to missing data
        assert!(report.risk_level >= RiskLevel::Low);
        assert!(report.metadata.contains_key("partial_analysis"));
        assert!(report.failed_checks.contains(&"holder_data_unavailable".to_string()));
        assert!(report.failed_checks.contains(&"liquidity_data_unavailable".to_string()));
    }

    #[test]
    fn test_partial_data_analysis_empty_mint() {
        let mut detector = RugDetector::new();

        // Should fail with empty mint
        let result = detector.analyze_with_partial_data(None, None, None, "");

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RugDetectorError::InsufficientData { .. }
        ));
    }

    #[test]
    fn test_partial_data_analysis_empty_holders() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let empty_holders: Vec<HolderInfo> = Vec::new();

        let result = detector.analyze_with_partial_data(
            Some(&token_data),
            Some(&empty_holders),
            None,
            &token_data.mint,
        );

        assert!(result.is_ok());
        let report = result.unwrap();
        assert!(report.failed_checks.contains(&"holder_data_empty".to_string()));
    }

    #[test]
    fn test_cache_invalidation() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        // First analysis
        let _ = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(detector.get_cached(&token_data.mint).is_some());

        // Invalidate cache
        detector.invalidate_cache(&token_data.mint);
        assert!(detector.get_cached(&token_data.mint).is_none());
    }

    #[test]
    fn test_invalidate_stale_cache() {
        let mut detector = RugDetector::with_config(RugDetectorConfig {
            cache_ttl_seconds: 300,
            ..Default::default()
        });

        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);
        let liquidity = create_test_liquidity();

        // First analysis
        let _ = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(!detector.token_cache.is_empty());

        // Invalidate with 0 duration (should clear everything)
        detector.invalidate_stale_cache(Duration::from_secs(0));
        assert!(detector.token_cache.is_empty());
    }

    #[test]
    fn test_low_liquidity_zero_minimum() {
        // Edge case: zero minimum in LowLiquidity warning
        let warning = RugWarning::LowLiquidity {
            liquidity_usd: 1000.0,
            minimum_usd: 0.0, // Zero minimum - should handle gracefully
        };

        // Should not panic, should return worst-case score
        let score = warning.severity_score();
        assert_eq!(score, 95);
    }

    #[test]
    fn test_low_holder_count_zero_minimum() {
        // Edge case: zero minimum in LowHolderCount warning
        let warning = RugWarning::LowHolderCount {
            count: 50,
            minimum: 0, // Zero minimum - should handle gracefully
        };

        // Should not panic, should return worst-case score
        let score = warning.severity_score();
        assert_eq!(score, 70);
    }

    #[test]
    fn test_new_error_types() {
        let err = RugDetectorError::PartialData {
            field: "holders".to_string(),
        };
        assert!(err.to_string().contains("holders"));

        let err = RugDetectorError::InvalidData {
            field: "percentage".to_string(),
            reason: "out of range".to_string(),
        };
        assert!(err.to_string().contains("percentage"));

        let err = RugDetectorError::DivisionByZero {
            context: "severity_score".to_string(),
        };
        assert!(err.to_string().contains("zero"));

        let err = RugDetectorError::AnalysisDegraded {
            reason: "missing holder data".to_string(),
        };
        assert!(err.to_string().contains("degraded"));
    }

    #[test]
    fn test_analyze_with_invalid_holder_percentages() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let liquidity = create_test_liquidity();

        // Holders with NaN percentage
        let holders = vec![HolderInfo {
            address: "Holder1".to_string(),
            amount: 1_000_000,
            percentage: f64::NAN, // Invalid
            is_creator: false,
            is_known_exchange: false,
        }];

        // Should not panic, should handle gracefully
        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        // Report should still be created
        assert!(!report.mint.is_empty());
    }

    #[test]
    fn test_analyze_with_negative_liquidity() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);

        // Negative liquidity (edge case)
        let liquidity = LiquidityInfo {
            pool_address: "Pool123".to_string(),
            lp_mint: "LPMint123".to_string(),
            liquidity_usd: -1000.0, // Negative - edge case
            token_amount: 0,
            quote_amount: 0,
            lp_burned: true,
            lp_holder: None,
            lp_locked_percentage: 100.0,
        };

        // Should not panic, should handle gracefully
        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        // Should flag as low liquidity
        assert!(report
            .warnings
            .iter()
            .any(|w| matches!(w, RugWarning::LowLiquidity { .. })));
    }

    #[test]
    fn test_partial_data_risk_bump() {
        let mut detector = RugDetector::lenient();

        // Even with lenient config, partial data should bump risk
        let result = detector.analyze_with_partial_data(
            None,
            None,
            None,
            "TestMint123",
        );

        assert!(result.is_ok());
        let report = result.unwrap();

        // Should be at least Low risk due to missing data
        assert!(report.risk_level >= RiskLevel::Low);
        assert!(report.risk_score >= 20);
    }

    #[test]
    fn test_analyze_without_liquidity() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let holders = create_test_holders(200, 1.5);

        // Analyze without liquidity data
        let report = detector.analyze_with_data(&token_data, &holders, None);

        // Should note missing liquidity
        assert!(report.failed_checks.contains(&"liquidity_data_missing".to_string()));
    }

    #[test]
    fn test_holder_with_zero_amount() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let liquidity = create_test_liquidity();

        // Holder with zero amount
        let holders = vec![
            HolderInfo {
                address: "Holder1".to_string(),
                amount: 0, // Zero amount
                percentage: 0.0,
                is_creator: false,
                is_known_exchange: false,
            },
            HolderInfo {
                address: "Holder2".to_string(),
                amount: 100_000,
                percentage: 1.0,
                is_creator: false,
                is_known_exchange: false,
            },
        ];

        // Should not panic
        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(report.holder_count == Some(2));
    }

    #[test]
    fn test_all_holders_are_exchanges() {
        let mut detector = RugDetector::new();
        let token_data = create_test_token_data();
        let liquidity = create_test_liquidity();

        // All holders are exchanges
        let holders: Vec<HolderInfo> = (0..100)
            .map(|i| HolderInfo {
                address: format!("Exchange{}", i),
                amount: 1_000_000,
                percentage: 1.0,
                is_creator: false,
                is_known_exchange: true, // All are exchanges
            })
            .collect();

        // Should not panic, exchanges are filtered for concentration checks
        let report = detector.analyze_with_data(&token_data, &holders, Some(&liquidity));
        assert!(!report.mint.is_empty());
    }
}
