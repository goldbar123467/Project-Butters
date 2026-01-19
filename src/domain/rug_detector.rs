//! Rug Pull Detector
//!
//! Detects potential rug pull risks by analyzing token holder distribution,
//! pool age, and authority settings. Helps avoid scam tokens.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Default minimum holder count for safety
pub const DEFAULT_MIN_HOLDER_COUNT: u64 = 100;

/// Default maximum percentage held by top 10 holders
pub const DEFAULT_MAX_TOP10_HOLDER_PCT: f64 = 70.0;

/// Default minimum pool age in hours
pub const DEFAULT_MIN_POOL_AGE_HOURS: u64 = 24;

#[derive(Error, Debug, Clone)]
pub enum RugDetectorError {
    #[error("Token info is incomplete: {0}")]
    IncompleteTokenInfo(String),

    #[error("Analysis failed: {0}")]
    AnalysisFailed(String),
}

/// Risk level for rug pull detection
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RugPullRisk {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

impl RugPullRisk {
    /// Returns a numeric score for the risk level (0-100)
    pub fn score(&self) -> u8 {
        match self {
            RugPullRisk::Safe => 0,
            RugPullRisk::Low => 25,
            RugPullRisk::Medium => 50,
            RugPullRisk::High => 75,
            RugPullRisk::Critical => 100,
        }
    }

    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            RugPullRisk::Safe => "Token appears safe based on analyzed metrics",
            RugPullRisk::Low => "Minor risk factors detected, proceed with caution",
            RugPullRisk::Medium => "Moderate risk factors, consider smaller position",
            RugPullRisk::High => "Significant rug pull indicators, avoid if possible",
            RugPullRisk::Critical => "Critical risk - likely scam or rug pull",
        }
    }
}

/// Token information used for rug pull analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    /// Token mint address
    pub mint: String,
    /// Token symbol
    pub symbol: String,
    /// Total number of unique holders
    pub holder_count: u64,
    /// Percentage of supply held by top 10 holders
    pub top10_holder_pct: f64,
    /// Pool creation timestamp (Unix seconds)
    pub pool_created_at: u64,
    /// Whether mint authority is revoked
    pub mint_authority_revoked: bool,
    /// Whether freeze authority is revoked
    pub freeze_authority_revoked: bool,
    /// Current timestamp for age calculation (Unix seconds)
    pub current_timestamp: u64,
    /// Liquidity in USD (optional, for additional checks)
    pub liquidity_usd: Option<f64>,
    /// Whether the token is verified/listed on aggregators
    pub is_verified: bool,
}

impl TokenInfo {
    /// Calculate pool age in hours
    pub fn pool_age_hours(&self) -> u64 {
        if self.current_timestamp > self.pool_created_at {
            (self.current_timestamp - self.pool_created_at) / 3600
        } else {
            0
        }
    }
}

/// Configuration for rug pull detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RugPullDetector {
    /// Minimum number of holders required (default: 100)
    pub min_holder_count: u64,
    /// Maximum percentage held by top 10 holders (default: 70%)
    pub max_top10_holder_pct: f64,
    /// Minimum pool age in hours (default: 24)
    pub min_pool_age_hours: u64,
    /// Whether mint authority must be revoked
    pub mint_authority_must_be_revoked: bool,
    /// Whether freeze authority must be revoked
    pub freeze_authority_must_be_revoked: bool,
    /// Minimum liquidity in USD (optional)
    pub min_liquidity_usd: Option<f64>,
    /// Whether to require token verification
    pub require_verification: bool,
}

impl Default for RugPullDetector {
    fn default() -> Self {
        Self {
            min_holder_count: DEFAULT_MIN_HOLDER_COUNT,
            max_top10_holder_pct: DEFAULT_MAX_TOP10_HOLDER_PCT,
            min_pool_age_hours: DEFAULT_MIN_POOL_AGE_HOURS,
            mint_authority_must_be_revoked: true,
            freeze_authority_must_be_revoked: true,
            min_liquidity_usd: Some(10_000.0),
            require_verification: false,
        }
    }
}

/// Detailed analysis result with breakdown of risk factors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RugAnalysisResult {
    /// Overall risk level
    pub risk: RugPullRisk,
    /// Individual risk factors found
    pub risk_factors: Vec<RiskFactor>,
    /// Recommendation
    pub recommendation: String,
}

/// Individual risk factor identified during analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskFactor {
    /// Name of the risk factor
    pub name: String,
    /// Severity contribution (0-25)
    pub severity: u8,
    /// Description of the risk
    pub description: String,
}

impl RugPullDetector {
    /// Create a new rug pull detector with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a strict detector with higher safety requirements
    pub fn strict() -> Self {
        Self {
            min_holder_count: 500,
            max_top10_holder_pct: 50.0,
            min_pool_age_hours: 72,
            mint_authority_must_be_revoked: true,
            freeze_authority_must_be_revoked: true,
            min_liquidity_usd: Some(50_000.0),
            require_verification: true,
        }
    }

    /// Create a lenient detector for higher risk tolerance
    pub fn lenient() -> Self {
        Self {
            min_holder_count: 50,
            max_top10_holder_pct: 85.0,
            min_pool_age_hours: 6,
            mint_authority_must_be_revoked: true,
            freeze_authority_must_be_revoked: false,
            min_liquidity_usd: Some(5_000.0),
            require_verification: false,
        }
    }

    /// Analyze a token and return its rug pull risk level
    pub fn analyze_token(&self, token_info: &TokenInfo) -> RugPullRisk {
        let result = self.analyze_token_detailed(token_info);
        result.risk
    }

    /// Perform detailed analysis with breakdown of risk factors
    pub fn analyze_token_detailed(&self, token_info: &TokenInfo) -> RugAnalysisResult {
        let mut risk_factors = Vec::new();
        let mut total_severity: u32 = 0;

        // Check holder count
        if token_info.holder_count < self.min_holder_count {
            let severity = if token_info.holder_count < 10 {
                25
            } else if token_info.holder_count < 50 {
                20
            } else {
                15
            };
            risk_factors.push(RiskFactor {
                name: "Low holder count".to_string(),
                severity,
                description: format!(
                    "Token has {} holders, minimum recommended is {}",
                    token_info.holder_count, self.min_holder_count
                ),
            });
            total_severity += severity as u32;
        }

        // Check top 10 holder concentration
        if token_info.top10_holder_pct > self.max_top10_holder_pct {
            let severity = if token_info.top10_holder_pct > 90.0 {
                25
            } else if token_info.top10_holder_pct > 80.0 {
                20
            } else {
                15
            };
            risk_factors.push(RiskFactor {
                name: "High holder concentration".to_string(),
                severity,
                description: format!(
                    "Top 10 holders control {:.1}% of supply, maximum recommended is {:.1}%",
                    token_info.top10_holder_pct, self.max_top10_holder_pct
                ),
            });
            total_severity += severity as u32;
        }

        // Check pool age
        let pool_age = token_info.pool_age_hours();
        if pool_age < self.min_pool_age_hours {
            let severity = if pool_age < 1 {
                25
            } else if pool_age < 6 {
                20
            } else {
                15
            };
            risk_factors.push(RiskFactor {
                name: "New pool".to_string(),
                severity,
                description: format!(
                    "Pool is only {} hours old, minimum recommended is {} hours",
                    pool_age, self.min_pool_age_hours
                ),
            });
            total_severity += severity as u32;
        }

        // Check mint authority
        if self.mint_authority_must_be_revoked && !token_info.mint_authority_revoked {
            risk_factors.push(RiskFactor {
                name: "Mint authority active".to_string(),
                severity: 25,
                description: "Token owner can mint unlimited new tokens".to_string(),
            });
            total_severity += 25;
        }

        // Check freeze authority
        if self.freeze_authority_must_be_revoked && !token_info.freeze_authority_revoked {
            risk_factors.push(RiskFactor {
                name: "Freeze authority active".to_string(),
                severity: 20,
                description: "Token owner can freeze any holder's tokens".to_string(),
            });
            total_severity += 20;
        }

        // Check liquidity
        if let Some(min_liq) = self.min_liquidity_usd {
            if let Some(liq) = token_info.liquidity_usd {
                if liq < min_liq {
                    let severity = if liq < 1_000.0 {
                        20
                    } else if liq < 5_000.0 {
                        15
                    } else {
                        10
                    };
                    risk_factors.push(RiskFactor {
                        name: "Low liquidity".to_string(),
                        severity,
                        description: format!(
                            "Pool has ${:.0} liquidity, minimum recommended is ${:.0}",
                            liq, min_liq
                        ),
                    });
                    total_severity += severity as u32;
                }
            }
        }

        // Check verification status
        if self.require_verification && !token_info.is_verified {
            risk_factors.push(RiskFactor {
                name: "Unverified token".to_string(),
                severity: 15,
                description: "Token is not verified on major aggregators".to_string(),
            });
            total_severity += 15;
        }

        // Determine risk level based on total severity
        let risk = if total_severity == 0 {
            RugPullRisk::Safe
        } else if total_severity <= 25 {
            RugPullRisk::Low
        } else if total_severity <= 50 {
            RugPullRisk::Medium
        } else if total_severity <= 75 {
            RugPullRisk::High
        } else {
            RugPullRisk::Critical
        };

        // Generate recommendation
        let recommendation = match risk {
            RugPullRisk::Safe => "Token passes all safety checks. Safe to trade.".to_string(),
            RugPullRisk::Low => "Minor concerns detected. Consider smaller position size.".to_string(),
            RugPullRisk::Medium => "Multiple risk factors present. Trade with caution and tight stops.".to_string(),
            RugPullRisk::High => "Significant rug pull risk. Avoid trading this token.".to_string(),
            RugPullRisk::Critical => "CRITICAL: This token shows strong indicators of being a scam. DO NOT TRADE.".to_string(),
        };

        RugAnalysisResult {
            risk,
            risk_factors,
            recommendation,
        }
    }

    /// Check if a token should be avoided based on its risk level
    pub fn should_avoid(&self, risk: RugPullRisk) -> bool {
        matches!(risk, RugPullRisk::High | RugPullRisk::Critical)
    }

    /// Check if a token should be avoided with custom threshold
    pub fn should_avoid_with_threshold(&self, risk: RugPullRisk, max_acceptable: RugPullRisk) -> bool {
        risk > max_acceptable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_safe_token() -> TokenInfo {
        TokenInfo {
            mint: "SafeToken111111111111111111111111111111111".to_string(),
            symbol: "SAFE".to_string(),
            holder_count: 1000,
            top10_holder_pct: 40.0,
            pool_created_at: 1000000,
            mint_authority_revoked: true,
            freeze_authority_revoked: true,
            current_timestamp: 1100000, // ~28 hours later
            liquidity_usd: Some(100_000.0),
            is_verified: true,
        }
    }

    fn create_risky_token() -> TokenInfo {
        TokenInfo {
            mint: "RiskyToken11111111111111111111111111111111".to_string(),
            symbol: "RISKY".to_string(),
            holder_count: 20,
            top10_holder_pct: 95.0,
            pool_created_at: 1099000, // Very new
            mint_authority_revoked: false,
            freeze_authority_revoked: false,
            current_timestamp: 1100000,
            liquidity_usd: Some(500.0),
            is_verified: false,
        }
    }

    #[test]
    fn test_safe_token_analysis() {
        let detector = RugPullDetector::new();
        let token = create_safe_token();
        let risk = detector.analyze_token(&token);
        assert_eq!(risk, RugPullRisk::Safe);
    }

    #[test]
    fn test_risky_token_analysis() {
        let detector = RugPullDetector::new();
        let token = create_risky_token();
        let risk = detector.analyze_token(&token);
        assert_eq!(risk, RugPullRisk::Critical);
    }

    #[test]
    fn test_should_avoid() {
        let detector = RugPullDetector::new();

        assert!(!detector.should_avoid(RugPullRisk::Safe));
        assert!(!detector.should_avoid(RugPullRisk::Low));
        assert!(!detector.should_avoid(RugPullRisk::Medium));
        assert!(detector.should_avoid(RugPullRisk::High));
        assert!(detector.should_avoid(RugPullRisk::Critical));
    }

    #[test]
    fn test_low_holder_count() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.holder_count = 50;

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "Low holder count"));
    }

    #[test]
    fn test_high_concentration() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.top10_holder_pct = 85.0;

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "High holder concentration"));
    }

    #[test]
    fn test_new_pool() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.pool_created_at = token.current_timestamp - 3600; // 1 hour old

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "New pool"));
    }

    #[test]
    fn test_mint_authority_not_revoked() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.mint_authority_revoked = false;

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "Mint authority active"));
        assert!(result.risk >= RugPullRisk::Low);
    }

    #[test]
    fn test_freeze_authority_not_revoked() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.freeze_authority_revoked = false;

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "Freeze authority active"));
    }

    #[test]
    fn test_low_liquidity() {
        let detector = RugPullDetector::new();
        let mut token = create_safe_token();
        token.liquidity_usd = Some(5_000.0);

        let result = detector.analyze_token_detailed(&token);
        assert!(result.risk_factors.iter().any(|f| f.name == "Low liquidity"));
    }

    #[test]
    fn test_strict_detector() {
        let detector = RugPullDetector::strict();
        let token = create_safe_token();

        // Safe token might not pass strict checks
        let result = detector.analyze_token_detailed(&token);
        // With strict settings, even "safe" tokens might show some risk
        assert!(result.risk_factors.is_empty() || result.risk <= RugPullRisk::Low);
    }

    #[test]
    fn test_lenient_detector() {
        let detector = RugPullDetector::lenient();
        let mut token = create_risky_token();
        // Make it slightly less risky
        token.holder_count = 60;
        token.top10_holder_pct = 80.0;
        token.pool_created_at = token.current_timestamp - 36000; // 10 hours
        token.mint_authority_revoked = true;
        token.liquidity_usd = Some(6_000.0);

        let result = detector.analyze_token_detailed(&token);
        // Lenient detector should be more accepting
        assert!(result.risk < RugPullRisk::Critical);
    }

    #[test]
    fn test_pool_age_calculation() {
        let token = TokenInfo {
            mint: "Test".to_string(),
            symbol: "TEST".to_string(),
            holder_count: 100,
            top10_holder_pct: 50.0,
            pool_created_at: 1000000,
            mint_authority_revoked: true,
            freeze_authority_revoked: true,
            current_timestamp: 1086400, // 24 hours later
            liquidity_usd: None,
            is_verified: false,
        };

        assert_eq!(token.pool_age_hours(), 24);
    }

    #[test]
    fn test_risk_score() {
        assert_eq!(RugPullRisk::Safe.score(), 0);
        assert_eq!(RugPullRisk::Low.score(), 25);
        assert_eq!(RugPullRisk::Medium.score(), 50);
        assert_eq!(RugPullRisk::High.score(), 75);
        assert_eq!(RugPullRisk::Critical.score(), 100);
    }

    #[test]
    fn test_risk_ordering() {
        assert!(RugPullRisk::Safe < RugPullRisk::Low);
        assert!(RugPullRisk::Low < RugPullRisk::Medium);
        assert!(RugPullRisk::Medium < RugPullRisk::High);
        assert!(RugPullRisk::High < RugPullRisk::Critical);
    }

    #[test]
    fn test_should_avoid_with_threshold() {
        let detector = RugPullDetector::new();

        // With Safe threshold, only Safe passes
        assert!(!detector.should_avoid_with_threshold(RugPullRisk::Safe, RugPullRisk::Safe));
        assert!(detector.should_avoid_with_threshold(RugPullRisk::Low, RugPullRisk::Safe));

        // With Medium threshold, Safe/Low/Medium pass
        assert!(!detector.should_avoid_with_threshold(RugPullRisk::Medium, RugPullRisk::Medium));
        assert!(detector.should_avoid_with_threshold(RugPullRisk::High, RugPullRisk::Medium));
    }

    #[test]
    fn test_detailed_analysis_recommendation() {
        let detector = RugPullDetector::new();

        let safe_token = create_safe_token();
        let safe_result = detector.analyze_token_detailed(&safe_token);
        assert!(safe_result.recommendation.contains("Safe to trade"));

        let risky_token = create_risky_token();
        let risky_result = detector.analyze_token_detailed(&risky_token);
        assert!(risky_result.recommendation.contains("DO NOT TRADE"));
    }
}
