//! Token bucket rate limiting algorithm.
//!
//! ## How it works
//!
//! 1. Bucket holds up to `capacity` tokens.
//! 2. Tokens refill at `refill_rate` per second.
//! 3. `try_consume(n)` returns Ok if n tokens available, Err otherwise.
//! 4. Time is tracked via `last_refill` timestamp for distributed correctness.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::LimiterError;

/// Configuration for a token bucket.
#[derive(Debug, Clone)]
pub struct TokenBucketConfig {
    /// Maximum tokens (burst capacity).
    pub capacity: u64,
    /// Tokens added per second (sustained rate).
    pub refill_rate: f64,
}

impl TokenBucketConfig {
    pub fn new(capacity: u64, refill_rate: f64) -> Result<Self, LimiterError> {
        if capacity == 0 {
            return Err(LimiterError::InvalidConfig("capacity must be > 0".into()));
        }
        if refill_rate <= 0.0 {
            return Err(LimiterError::InvalidConfig(
                "refill_rate must be > 0".into(),
            ));
        }
        Ok(Self {
            capacity,
            refill_rate,
        })
    }
}

/// Serializable state for a token bucket (for Redis storage).
/// Uses seconds + subsecond nanos for precision.
#[derive(Debug, Clone)]
pub struct BucketState {
    pub tokens: f64,
    /// Unix timestamp (secs) + fractional part for sub-second precision.
    pub last_refill_secs: u64,
    pub last_refill_nanos: u32,
}

impl BucketState {
    pub fn to_storage(&self) -> String {
        format!(
            "{}:{}:{}",
            self.tokens, self.last_refill_secs, self.last_refill_nanos
        )
    }

    pub fn from_storage(s: &str) -> Result<Self, LimiterError> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err(LimiterError::Storage("invalid bucket state format".into()));
        }
        let tokens: f64 = parts[0]
            .parse()
            .map_err(|_| LimiterError::Storage("invalid tokens".into()))?;
        let last_refill_secs: u64 = parts[1]
            .parse()
            .map_err(|_| LimiterError::Storage("invalid last_refill_secs".into()))?;
        let last_refill_nanos: u32 = parts[2]
            .parse()
            .map_err(|_| LimiterError::Storage("invalid last_refill_nanos".into()))?;
        Ok(Self {
            tokens,
            last_refill_secs,
            last_refill_nanos,
        })
    }

    fn elapsed_secs(&self, now_secs: u64, now_nanos: u32) -> f64 {
        let now_total = now_secs as f64 + now_nanos as f64 / 1e9;
        let then_total = self.last_refill_secs as f64 + self.last_refill_nanos as f64 / 1e9;
        (now_total - then_total).max(0.0)
    }
}

/// In-memory token bucket (used for local computation after fetching state).
pub struct TokenBucket {
    config: TokenBucketConfig,
    state: BucketState,
}

impl TokenBucket {
    pub fn new(config: TokenBucketConfig) -> Self {
        let (secs, nanos) = now_secs_nanos();
        Self {
            config: config.clone(),
            state: BucketState {
                tokens: config.capacity as f64,
                last_refill_secs: secs,
                last_refill_nanos: nanos,
            },
        }
    }

    /// Restore from stored state (e.g. from Redis).
    pub fn from_state(config: TokenBucketConfig, state: BucketState) -> Self {
        Self { config, state }
    }

    /// Refill tokens based on elapsed time since last_refill.
    fn refill(&mut self) {
        let (now_secs, now_nanos) = now_secs_nanos();
        let elapsed = self.state.elapsed_secs(now_secs, now_nanos);
        let added = elapsed * self.config.refill_rate;
        self.state.tokens = (self.state.tokens + added).min(self.config.capacity as f64);
        self.state.last_refill_secs = now_secs;
        self.state.last_refill_nanos = now_nanos;
    }

    /// Try to consume `n` tokens. Returns Ok(()) if allowed, Err if denied.
    pub fn try_consume(&mut self, n: u64) -> Result<(), LimiterError> {
        if n == 0 {
            return Ok(());
        }
        if n > self.config.capacity {
            return Err(LimiterError::InvalidConfig(
                "requested tokens exceed capacity".into(),
            ));
        }
        self.refill();
        let n_f = n as f64;
        if self.state.tokens >= n_f {
            self.state.tokens -= n_f;
            Ok(())
        } else {
            Err(LimiterError::RateLimited)
        }
    }

    /// Consume 1 token (common case).
    pub fn allow(&mut self) -> Result<(), LimiterError> {
        self.try_consume(1)
    }

    /// Get current state for persistence.
    pub fn state(&self) -> &BucketState {
        &self.state
    }

    /// Get state after refill (for accurate persistence).
    pub fn state_after_refill(&mut self) -> BucketState {
        self.refill();
        self.state.clone()
    }
}

fn now_secs_nanos() -> (u64, u32) {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    (d.as_secs(), d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_within_capacity() {
        let config = TokenBucketConfig::new(10, 1.0).unwrap();
        let mut bucket = TokenBucket::new(config);
        for _ in 0..10 {
            assert!(bucket.allow().is_ok());
        }
        assert!(bucket.allow().is_err());
    }

    #[test]
    fn refill_over_time() {
        let config = TokenBucketConfig::new(2, 10.0).unwrap();
        let mut bucket = TokenBucket::new(config);
        assert!(bucket.allow().is_ok());
        assert!(bucket.allow().is_ok());
        assert!(bucket.allow().is_err());
        std::thread::sleep(Duration::from_millis(150));
        assert!(bucket.allow().is_ok());
    }

    #[test]
    fn state_roundtrip() {
        let config = TokenBucketConfig::new(5, 1.0).unwrap();
        let mut bucket = TokenBucket::new(config);
        bucket.allow().unwrap();
        let state = bucket.state_after_refill();
        let s = state.to_storage();
        let restored = BucketState::from_storage(&s).unwrap();
        assert_eq!(restored.tokens, state.tokens);
        assert_eq!(restored.last_refill_secs, state.last_refill_secs);
        assert_eq!(restored.last_refill_nanos, state.last_refill_nanos);
    }
}
