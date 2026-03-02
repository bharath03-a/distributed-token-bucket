//! # Limiter Core
//!
//! Token bucket rate limiting algorithm and consistent hashing.
//!
//! ## Token Bucket Algorithm
//!
//! A token bucket has:
//! - **capacity**: max tokens (burst limit)
//! - **refill_rate**: tokens added per second
//! - **tokens**: current available tokens
//!
//! Each request consumes 1 token. Tokens refill over time. If tokens < 1, request is denied.

mod bucket;
mod consistent_hash;
mod error;

pub use bucket::{BucketState, TokenBucket, TokenBucketConfig};
pub use consistent_hash::ConsistentHashRing;
pub use error::LimiterError;
