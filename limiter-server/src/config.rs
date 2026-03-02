//! Configuration from environment.

use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub redis_url: String,
    pub listen_addr: String,
    pub default_capacity: u64,
    pub default_refill_rate: f64,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            redis_url: env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into()),
            listen_addr: env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:50051".into()),
            default_capacity: env::var("DEFAULT_CAPACITY")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            default_refill_rate: env::var("DEFAULT_REFILL_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10.0),
        }
    }
}
