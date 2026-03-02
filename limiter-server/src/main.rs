//! Distributed rate limiter server.
//!
//! gRPC server + Redis backend + consistent hashing for horizontal scaling.

mod config;
mod lua;
mod service;

use std::sync::Arc;

use limiter_core::ConsistentHashRing;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use service::LimiterServiceImpl;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env();
    tracing::info!(?config, "starting limiter server");

    let redis_url = config.redis_url.clone();
    let client = redis::Client::open(redis_url.clone())?;

    // For single Redis, we use one "node" in the ring. For multiple Redis shards,
    // add each URL to the ring and use consistent hashing to pick one.
    let ring: Arc<ConsistentHashRing<String>> = Arc::new(ConsistentHashRing::new());
    ring.add_node(config.redis_url.clone());

    let svc = LimiterServiceImpl::new(client, ring, config.default_capacity, config.default_refill_rate);

    let addr = config.listen_addr.parse()?;
    tracing::info!(%addr, "listening");

    tonic::transport::Server::builder()
        .add_service(service::limiter::limiter_service_server::LimiterServiceServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
