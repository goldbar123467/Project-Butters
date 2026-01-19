use std::collections::HashMap;
use thiserror::Error;
use serde::{Deserialize, Serialize};

#[derive(Debug, Error)]
pub enum RiskViolation {
    #[error("Position size {0} exceeds maximum allowed {1}")]
    PositionSizeExceeded(f64, f64),
    
    #[error("Exposure {0} exceeds maximum allowed {1}")]
    ExposureExceeded(f64, f64),
    
    #[error("Risk limit validation failed: {0}")]
    ValidationFailed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskLimits {
    pub max_position_size: f64,
    pub max_exposure: f64,
    pub max_leverage: f64,
    pub per_instrument_limits: HashMap<String, InstrumentRiskLimits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentRiskLimits {
    pub max_position_size: f64,
    pub max_exposure: f64,
}

pub trait RiskCheck {
    fn validate_position_size(&self, size: f64) -> Result<(), RiskViolation>;
    fn validate_exposure(&self, exposure: f64) -> Result<(), RiskViolation>;
    fn validate_leverage(&self, leverage: f64) -> Result<(), RiskViolation>;
}

impl RiskCheck for RiskLimits {
    fn validate_position_size(&self, size: f64) -> Result<(), RiskViolation> {
        if size.abs() > self.max_position_size {
            Err(RiskViolation::PositionSizeExceeded(size, self.max_position_size))
        } else {
            Ok(())
        }
    }

    fn validate_exposure(&self, exposure: f64) -> Result<(), RiskViolation> {
        if exposure.abs() > self.max_exposure {
            Err(RiskViolation::ExposureExceeded(exposure, self.max_exposure))
        } else {
            Ok(())
        }
    }

    fn validate_leverage(&self, leverage: f64) -> Result<(), RiskViolation> {
        if leverage.abs() > self.max_leverage {
            Err(RiskViolation::ValidationFailed(
                format!("Leverage {} exceeds maximum allowed {}", leverage, self.max_leverage)
            ))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_position_size_validation() {
        let limits = RiskLimits {
            max_position_size: 100.0,
            max_exposure: 1000.0,
            max_leverage: 10.0,
            per_instrument_limits: HashMap::new(),
        };

        assert!(limits.validate_position_size(50.0).is_ok());
        assert!(limits.validate_position_size(-50.0).is_ok());
        assert!(limits.validate_position_size(101.0).is_err());
        assert!(limits.validate_position_size(-101.0).is_err());
    }

    #[test]
    fn test_exposure_validation() {
        let limits = RiskLimits {
            max_position_size: 100.0,
            max_exposure: 1000.0,
            max_leverage: 10.0,
            per_instrument_limits: HashMap::new(),
        };

        assert!(limits.validate_exposure(500.0).is_ok());
        assert!(limits.validate_exposure(-500.0).is_ok());
        assert!(limits.validate_exposure(1001.0).is_err());
        assert!(limits.validate_exposure(-1001.0).is_err());
    }

    #[test]
    fn test_leverage_validation() {
        let limits = RiskLimits {
            max_position_size: 100.0,
            max_exposure: 1000.0,
            max_leverage: 10.0,
            per_instrument_limits: HashMap::new(),
        };

        assert!(limits.validate_leverage(5.0).is_ok());
        assert!(limits.validate_leverage(-5.0).is_ok());
        assert!(limits.validate_leverage(11.0).is_err());
        assert!(limits.validate_leverage(-11.0).is_err());
    }

    #[test]
    fn test_instrument_specific_limits() {
        let mut per_instrument = HashMap::new();
        per_instrument.insert("BTC".to_string(), InstrumentRiskLimits {
            max_position_size: 10.0,
            max_exposure: 100.0,
        });

        let limits = RiskLimits {
            max_position_size: 100.0,
            max_exposure: 1000.0,
            max_leverage: 10.0,
            per_instrument_limits: per_instrument,
        };

        // Should use instrument-specific limits when available
        let btc_limits = limits.per_instrument_limits.get("BTC").unwrap();
        assert!(btc_limits.max_position_size == 10.0);
        assert!(btc_limits.max_exposure == 100.0);
    }
}