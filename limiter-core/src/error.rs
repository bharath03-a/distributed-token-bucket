//! Error types for the rate limiter.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum LimiterError {
    #[error("invalid config: {0}")]
    InvalidConfig(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("rate limited")]
    RateLimited,
}
