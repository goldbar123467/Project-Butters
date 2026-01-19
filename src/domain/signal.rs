use std::collections::HashMap;
use std::fmt;

/// Enum representing different types of signals
#[derive(Debug, Clone, PartialEq)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
    Custom(String),
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalType::Buy => write!(f, "Buy"),
            SignalType::Sell => write!(f, "Sell"),
            SignalType::Hold => write!(f, "Hold"),
            SignalType::Custom(s) => write!(f, "Custom({})", s),
        }
    }
}

/// Represents a trading signal with type, strength and confidence
#[derive(Debug, Clone)]
pub struct Signal {
    pub signal_type: SignalType,
    pub value: f64,
    pub z_score: f64,
    pub confidence: f64,
    pub metadata: HashMap<String, String>,
}

impl Signal {
    /// Creates a new signal with default confidence calculation
    pub fn new(signal_type: SignalType, value: f64, z_score: f64) -> Self {
        let confidence = Self::calculate_confidence(z_score);
        Self {
            signal_type,
            value,
            z_score,
            confidence,
            metadata: HashMap::new(),
        }
    }

    /// Calculates confidence based on z-score using standard normal CDF
    /// Confidence ranges from 0.0 to 1.0
    pub fn calculate_confidence(z_score: f64) -> f64 {
        use statrs::function::erf::erf;
        // Standard normal CDF: Φ(z) = 0.5 * (1 + erf(z / sqrt(2)))
        0.5 * (1.0 + erf(z_score / f64::sqrt(2.0)))
    }

    /// Validates the signal meets basic criteria
    pub fn validate(&self) -> Result<(), String> {
        if self.confidence.is_nan() || self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(format!("Invalid confidence value: {}", self.confidence));
        }

        if self.z_score.is_nan() {
            return Err("Z-score cannot be NaN".to_string());
        }

        Ok(())
    }

    /// Adds metadata to the signal
    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_signal_creation() {
        let signal = Signal::new(SignalType::Buy, 1.5, 2.0);
        assert_eq!(signal.signal_type, SignalType::Buy);
        assert_eq!(signal.value, 1.5);
        assert_eq!(signal.z_score, 2.0);
    }

    #[test]
    fn test_confidence_calculation() {
        // Test known z-score to confidence mappings
        assert_relative_eq!(Signal::calculate_confidence(0.0), 0.5, epsilon = 0.001);
        assert_relative_eq!(Signal::calculate_confidence(1.0), 0.841, epsilon = 0.001);
        assert_relative_eq!(Signal::calculate_confidence(2.0), 0.977, epsilon = 0.001);
        assert_relative_eq!(Signal::calculate_confidence(3.0), 0.998, epsilon = 0.001);
    }

    #[test]
    fn test_signal_validation() {
        let valid_signal = Signal::new(SignalType::Sell, -1.2, -1.5);
        assert!(valid_signal.validate().is_ok());

        let mut invalid_conf_signal = Signal::new(SignalType::Hold, 0.1, 0.5);
        invalid_conf_signal.confidence = 1.1;
        assert!(invalid_conf_signal.validate().is_err());

        let mut nan_z_signal = Signal::new(SignalType::Buy, 1.0, 0.0);
        nan_z_signal.z_score = f64::NAN;
        assert!(nan_z_signal.validate().is_err());
    }

    #[test]
    fn test_metadata_operations() {
        let mut signal = Signal::new(SignalType::Custom("arbitrage".to_string()), 0.8, 1.2);
        signal.add_metadata("source".to_string(), "strategy_x".to_string());
        signal.add_metadata("timestamp".to_string(), "2026-01-05T12:00:00Z".to_string());

        assert_eq!(signal.metadata.len(), 2);
        assert_eq!(signal.metadata.get("source"), Some(&"strategy_x".to_string()));
    }
}