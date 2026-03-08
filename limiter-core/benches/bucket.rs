//! Criterion benchmarks for the token bucket hot path.
//!
//! These measure the pure in-memory algorithm cost — no Redis, no network.
//! The goal is to verify that the Rust-side computation is negligible compared
//! to the Redis round-trip (~0.1ms LAN), so the bottleneck is always Redis
//! throughput, not CPU.
//!
//! Run with: cargo bench -p limiter-core
//! HTML report: limiter-core/target/criterion/report/index.html

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use limiter_core::{BucketState, ConsistentHashRing, TokenBucket, TokenBucketConfig};

fn bench_allow(c: &mut Criterion) {
    c.bench_function("TokenBucket::allow (single token, in-memory)", |b| {
        let config = TokenBucketConfig::new(1_000_000, 1_000_000.0).unwrap();
        let mut bucket = TokenBucket::new(config);
        b.iter(|| {
            black_box(bucket.allow()).ok();
        });
    });
}

fn bench_try_consume(c: &mut Criterion) {
    c.bench_function("TokenBucket::try_consume (10 tokens, in-memory)", |b| {
        let config = TokenBucketConfig::new(1_000_000, 1_000_000.0).unwrap();
        let mut bucket = TokenBucket::new(config);
        b.iter(|| {
            black_box(bucket.try_consume(10)).ok();
        });
    });
}

fn bench_state_serialization(c: &mut Criterion) {
    // Serialization runs on every Redis write — keep it cheap.
    c.bench_function("BucketState::to_storage (serialize)", |b| {
        let state = BucketState {
            tokens: 42.5,
            last_refill_secs: 1_700_000_000,
            last_refill_nanos: 123_456_789,
        };
        b.iter(|| {
            black_box(state.to_storage());
        });
    });

    c.bench_function("BucketState::from_storage (deserialize)", |b| {
        let raw = "42.500000:1700000000:123456789:100:10.000000";
        b.iter(|| {
            black_box(BucketState::from_storage(raw)).ok();
        });
    });
}

fn bench_consistent_hash(c: &mut Criterion) {
    // get_node runs on every request to route to the correct Redis shard.
    c.bench_function("ConsistentHashRing::get_node (3 nodes)", |b| {
        let ring = ConsistentHashRing::new();
        ring.add_node("redis://shard-1:6379".to_string());
        ring.add_node("redis://shard-2:6379".to_string());
        ring.add_node("redis://shard-3:6379".to_string());
        b.iter(|| {
            black_box(ring.get_node(black_box("user:123")));
        });
    });
}

criterion_group!(
    benches,
    bench_allow,
    bench_try_consume,
    bench_state_serialization,
    bench_consistent_hash,
);
criterion_main!(benches);
