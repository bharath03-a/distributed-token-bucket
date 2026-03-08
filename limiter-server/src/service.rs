//! gRPC service implementation.
//!
//! Uses Redis Lua script for atomic token bucket operations (no race conditions).

use std::sync::Arc;

use limiter_core::{BucketState, ConsistentHashRing, TokenBucketConfig};
use redis::AsyncCommands;
use tonic::{Request, Response, Status};
use tracing::instrument;

pub mod limiter {
    tonic::include_proto!("limiter");
}

use limiter::{limiter_service_server::LimiterService, AllowRequest, AllowResponse, SetLimitRequest, SetLimitResponse};

pub struct LimiterServiceImpl {
    redis: redis::Client,
    #[allow(dead_code)] // Used when multiple Redis shards
    ring: Arc<ConsistentHashRing<String>>,
    default_capacity: u64,
    default_refill_rate: f64,
}

const KEY_PREFIX: &str = "limiter:";

impl LimiterServiceImpl {
    pub fn new(
        redis: redis::Client,
        ring: Arc<ConsistentHashRing<String>>,
        default_capacity: u64,
        default_refill_rate: f64,
    ) -> Self {
        Self {
            redis,
            ring,
            default_capacity,
            default_refill_rate,
        }
    }

    fn redis_key(&self, key: &str) -> String {
        format!("{}{}", KEY_PREFIX, key)
    }

    async fn get_conn(&self) -> Result<redis::aio::MultiplexedConnection, Status> {
        self.redis
            .get_multiplexed_tokio_connection()
            .await
            .map_err(|e| Status::internal(format!("Redis connection failed: {}", e)))
    }

}

#[tonic::async_trait]
impl LimiterService for LimiterServiceImpl {
    #[instrument(skip(self, request))]
    async fn allow(&self, request: Request<AllowRequest>) -> Result<Response<AllowResponse>, Status> {
        let req = request.into_inner();
        let key = req.key;
        let tokens = req.tokens.max(1);

        let redis_key = self.redis_key(&key);
        let mut conn = self.get_conn().await?;

        let script = redis::Script::new(crate::lua::ALLOW_SCRIPT);
        let result: i32 = script
            .key(&redis_key)
            .arg(self.default_capacity)
            .arg(self.default_refill_rate)
            .arg(tokens)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| Status::internal(format!("Redis script failed: {}", e)))?;

        Ok(Response::new(if result == 1 {
            AllowResponse { allowed: true, message: String::new() }
        } else {
            AllowResponse { allowed: false, message: "rate limited".into() }
        }))
    }

    async fn set_limit(&self, request: Request<SetLimitRequest>) -> Result<Response<SetLimitResponse>, Status> {
        let req = request.into_inner();
        let config = TokenBucketConfig::new(req.capacity, req.refill_rate)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let redis_key = self.redis_key(&req.key);
        let mut conn = self.get_conn().await?;

        let raw: Option<String> = conn.get(&redis_key).await.map_err(|e| {
            Status::internal(format!("Redis GET failed: {}", e))
        })?;

        let state = if let Some(s) = raw {
            let parts: Vec<&str> = s.split(':').collect();
            if parts.len() >= 3 {
                BucketState {
                    tokens: parts[0].parse().unwrap_or(config.capacity as f64),
                    last_refill_secs: parts[1].parse().unwrap_or(0),
                    last_refill_nanos: parts[2].parse().unwrap_or(0),
                }
            } else {
                BucketState {
                    tokens: config.capacity as f64,
                    last_refill_secs: 0,
                    last_refill_nanos: 0,
                }
            }
        } else {
            let (secs, nanos) = {
                let n = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap();
                (n.as_secs(), n.subsec_nanos())
            };
            BucketState {
                tokens: config.capacity as f64,
                last_refill_secs: secs,
                last_refill_nanos: nanos,
            }
        };

        let storage = format!(
            "{}:{}:{}:{}:{}",
            state.tokens,
            state.last_refill_secs,
            state.last_refill_nanos,
            config.capacity,
            config.refill_rate,
        );

        conn.set_ex::<_, _, ()>(&redis_key, &storage, 86400)
            .await
            .map_err(|e| Status::internal(format!("Redis SET failed: {}", e)))?;

        Ok(Response::new(SetLimitResponse { ok: true, error: String::new() }))
    }
}
