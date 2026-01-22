//! Honeypot Analysis Cache
//!
//! Asymmetric TTL cache for honeypot analysis results.
//! - Safe tokens: shorter TTL (may change)
//! - Honeypot tokens: longer TTL (status is stable)

use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::domain::honeypot_detector::HoneypotAnalysis;

/// Cache entry with TTL tracking
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub analysis: HoneypotAnalysis,
    pub inserted_at: Instant,
    pub ttl: Duration,
}

impl CacheEntry {
    /// Create a new cache entry
    pub fn new(analysis: HoneypotAnalysis, ttl: Duration) -> Self {
        Self {
            analysis,
            inserted_at: Instant::now(),
            ttl,
        }
    }

    /// Check if entry is still valid
    pub fn is_valid(&self) -> bool {
        self.inserted_at.elapsed() < self.ttl
    }

    /// Get time remaining before expiry
    pub fn time_remaining(&self) -> Option<Duration> {
        let elapsed = self.inserted_at.elapsed();
        if elapsed < self.ttl {
            Some(self.ttl - elapsed)
        } else {
            None
        }
    }
}

/// Asymmetric TTL cache for honeypot analysis
///
/// Safe tokens have shorter TTL because their status may change.
/// Honeypot tokens have longer TTL because once blocked, they stay blocked.
#[derive(Debug)]
pub struct HoneypotCache {
    entries: HashMap<Pubkey, CacheEntry>,
    /// TTL for safe tokens (shorter - may change)
    safe_ttl: Duration,
    /// TTL for honeypot tokens (longer - status is stable)
    honeypot_ttl: Duration,
    /// Maximum entries before cleanup
    max_entries: usize,
}

impl HoneypotCache {
    /// Default TTL for safe tokens (5 minutes)
    pub const DEFAULT_SAFE_TTL: Duration = Duration::from_secs(300);
    /// Default TTL for honeypot tokens (1 hour)
    pub const DEFAULT_HONEYPOT_TTL: Duration = Duration::from_secs(3600);
    /// Default max cache entries
    pub const DEFAULT_MAX_ENTRIES: usize = 10000;

    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self::with_config(
            Self::DEFAULT_SAFE_TTL,
            Self::DEFAULT_HONEYPOT_TTL,
            Self::DEFAULT_MAX_ENTRIES,
        )
    }

    /// Create a new cache with custom TTL settings
    pub fn with_config(safe_ttl: Duration, honeypot_ttl: Duration, max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            safe_ttl,
            honeypot_ttl,
            max_entries,
        }
    }

    /// Insert an analysis into the cache
    ///
    /// TTL is determined by whether the token should be blocked:
    /// - Blocked tokens get longer TTL (honeypot_ttl)
    /// - Safe tokens get shorter TTL (safe_ttl)
    pub fn insert(&mut self, mint: Pubkey, analysis: HoneypotAnalysis) {
        // Cleanup if we're at capacity
        if self.entries.len() >= self.max_entries {
            self.cleanup();
        }

        // Still at capacity after cleanup? Remove oldest entry
        if self.entries.len() >= self.max_entries {
            self.remove_oldest();
        }

        // Determine TTL based on risk level
        let ttl = if analysis.risk_level.should_block() {
            self.honeypot_ttl
        } else {
            self.safe_ttl
        };

        let entry = CacheEntry::new(analysis, ttl);
        self.entries.insert(mint, entry);
    }

    /// Get a cached analysis if valid
    pub fn get(&self, mint: &Pubkey) -> Option<&HoneypotAnalysis> {
        self.entries
            .get(mint)
            .filter(|entry| entry.is_valid())
            .map(|entry| &entry.analysis)
    }

    /// Get a cached entry with metadata if valid
    pub fn get_entry(&self, mint: &Pubkey) -> Option<&CacheEntry> {
        self.entries.get(mint).filter(|entry| entry.is_valid())
    }

    /// Check if a valid entry exists
    pub fn contains(&self, mint: &Pubkey) -> bool {
        self.get(mint).is_some()
    }

    /// Remove an entry from the cache
    pub fn remove(&mut self, mint: &Pubkey) -> Option<HoneypotAnalysis> {
        self.entries.remove(mint).map(|e| e.analysis)
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Remove expired entries
    pub fn cleanup(&mut self) {
        self.entries.retain(|_, entry| entry.is_valid());
    }

    /// Remove the oldest entry
    fn remove_oldest(&mut self) {
        if let Some(oldest_key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.inserted_at)
            .map(|(key, _)| *key)
        {
            self.entries.remove(&oldest_key);
        }
    }

    /// Get the number of entries (including expired)
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the number of valid entries
    pub fn valid_count(&self) -> usize {
        self.entries.values().filter(|e| e.is_valid()).count()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let total = self.entries.len();
        let valid = self.valid_count();
        let safe_count = self
            .entries
            .values()
            .filter(|e| e.is_valid() && !e.analysis.risk_level.should_block())
            .count();
        let blocked_count = self
            .entries
            .values()
            .filter(|e| e.is_valid() && e.analysis.risk_level.should_block())
            .count();

        CacheStats {
            total_entries: total,
            valid_entries: valid,
            expired_entries: total - valid,
            safe_entries: safe_count,
            blocked_entries: blocked_count,
        }
    }
}

impl Default for HoneypotCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_entries: usize,
    pub valid_entries: usize,
    pub expired_entries: usize,
    pub safe_entries: usize,
    pub blocked_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::honeypot_detector::HoneypotRisk;

    fn create_test_mint() -> Pubkey {
        Pubkey::new_unique()
    }

    fn create_safe_analysis() -> HoneypotAnalysis {
        HoneypotAnalysis::safe()
    }

    fn create_blocked_analysis() -> HoneypotAnalysis {
        HoneypotAnalysis {
            risk_level: HoneypotRisk::High,
            can_transfer: false,
            issues: vec!["Test honeypot".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = HoneypotCache::new();
        let mint = create_test_mint();
        let analysis = create_safe_analysis();

        cache.insert(mint, analysis.clone());

        let cached = cache.get(&mint);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().risk_level, HoneypotRisk::Safe);
    }

    #[test]
    fn test_cache_asymmetric_ttl() {
        let safe_ttl = Duration::from_millis(50);
        let honeypot_ttl = Duration::from_millis(200);
        let mut cache = HoneypotCache::with_config(safe_ttl, honeypot_ttl, 100);

        let safe_mint = create_test_mint();
        let blocked_mint = create_test_mint();

        cache.insert(safe_mint, create_safe_analysis());
        cache.insert(blocked_mint, create_blocked_analysis());

        // Get entries to check TTL
        let safe_entry = cache.get_entry(&safe_mint).unwrap();
        let blocked_entry = cache.get_entry(&blocked_mint).unwrap();

        assert_eq!(safe_entry.ttl, safe_ttl);
        assert_eq!(blocked_entry.ttl, honeypot_ttl);
    }

    #[test]
    fn test_cache_expiry() {
        let short_ttl = Duration::from_millis(10);
        let mut cache = HoneypotCache::with_config(short_ttl, short_ttl, 100);

        let mint = create_test_mint();
        cache.insert(mint, create_safe_analysis());

        // Should be valid immediately
        assert!(cache.contains(&mint));

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(20));

        // Should be expired now
        assert!(!cache.contains(&mint));
    }

    #[test]
    fn test_cache_cleanup() {
        let short_ttl = Duration::from_millis(10);
        let mut cache = HoneypotCache::with_config(short_ttl, short_ttl, 100);

        // Insert some entries
        for _ in 0..5 {
            cache.insert(create_test_mint(), create_safe_analysis());
        }

        assert_eq!(cache.len(), 5);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(20));

        // Cleanup
        cache.cleanup();

        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_cache_max_entries() {
        let mut cache = HoneypotCache::with_config(
            Duration::from_secs(60),
            Duration::from_secs(60),
            3, // Very small max
        );

        // Insert more than max
        for _ in 0..5 {
            cache.insert(create_test_mint(), create_safe_analysis());
        }

        // Should be at or below max
        assert!(cache.len() <= 3);
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = HoneypotCache::new();

        cache.insert(create_test_mint(), create_safe_analysis());
        cache.insert(create_test_mint(), create_safe_analysis());
        cache.insert(create_test_mint(), create_blocked_analysis());

        let stats = cache.stats();

        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.valid_entries, 3);
        assert_eq!(stats.safe_entries, 2);
        assert_eq!(stats.blocked_entries, 1);
    }

    #[test]
    fn test_cache_remove() {
        let mut cache = HoneypotCache::new();
        let mint = create_test_mint();

        cache.insert(mint, create_safe_analysis());
        assert!(cache.contains(&mint));

        let removed = cache.remove(&mint);
        assert!(removed.is_some());
        assert!(!cache.contains(&mint));
    }

    #[test]
    fn test_cache_entry_time_remaining() {
        let ttl = Duration::from_millis(100);
        let entry = CacheEntry::new(create_safe_analysis(), ttl);

        let remaining = entry.time_remaining();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() <= ttl);

        // Wait for expiry
        std::thread::sleep(Duration::from_millis(110));

        let remaining = entry.time_remaining();
        assert!(remaining.is_none());
    }
}
