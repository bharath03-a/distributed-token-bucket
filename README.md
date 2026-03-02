# Distributed Token Bucket Rate Limiter

A high-performance distributed rate-limiting system built in Rust. Handles 100k+ requests/sec using token bucket algorithms, Redis for shared state, gRPC for the API, and consistent hashing for horizontal scaling.

## Key Concepts (Distributed Systems SWE)

### 1. Token Bucket Algorithm
- **Capacity**: Max tokens (burst limit). Allows short bursts above sustained rate.
- **Refill rate**: Tokens added per second (sustained throughput).
- Each request consumes 1 token. If tokens ≥ 1 → allow; else → deny.
- **Why**: Smooths traffic, allows bursts, predictable behavior. Alternative: leaky bucket (strict rate, no burst).

### 2. Consistent Hashing
- Maps keys (e.g. `user:123`) to Redis shards.
- **Problem**: With `key % N` nodes, adding/removing a node remaps ~all keys → thundering herd.
- **Solution**: Ring of hash space; each node owns a range. Adding a node remaps ~1/N keys.
- **Virtual nodes**: 150 vnodes per physical node for even distribution.

### 3. Atomicity (Redis Lua)
- Read-modify-write is not atomic: two requests can both read 1 token, both consume, both write → double spend.
- **Solution**: Lua script runs atomically on Redis. Single GET → compute → SET in one shot.

### 4. gRPC
- Binary protocol, HTTP/2 multiplexing, strong typing via protobuf.
- Sub-millisecond latency, efficient for high-throughput services.

### 5. Multithreading
- Tokio multi-threaded runtime (4 workers). Each request is async; many concurrent requests handled efficiently.

## Architecture

```
┌─────────────┐     gRPC      ┌─────────────────┐     Redis      ┌───────┐
│   Clients   │ ◄──────────► │  Limiter Server │ ◄──────────► │ Redis │
│ (API, CLI)  │               │  (multi-thread) │               │       │
└─────────────┘               └─────────────────┘               └───────┘
                                       │
                                       │ Consistent hashing (for multi-shard)
                                       ▼
                              ┌─────────────────┐
                              │ Redis Shard 2   │  (future)
                              └─────────────────┘
```

## Quick Start

### Prerequisites
- Rust 1.70+
- Redis 6+

### Run

```bash
# Terminal 1: Start Redis (Docker)
docker run -d -p 6379:6379 redis:7-alpine

# Terminal 2: Start server
cargo run -p limiter-server

# Terminal 3: Use client
cargo run -p limiter-client -- allow --key user:123
cargo run -p limiter-client -- set-limit --key user:123 --capacity 10 --refill-rate 2
```

### Environment
| Var | Default | Description |
|-----|---------|-------------|
| `REDIS_URL` | `redis://127.0.0.1:6379` | Redis connection |
| `LISTEN_ADDR` | `0.0.0.0:50051` | gRPC listen address |
| `DEFAULT_CAPACITY` | `100` | Default bucket capacity |
| `DEFAULT_REFILL_RATE` | `10.0` | Default tokens/sec |

## Project Structure

```
├── limiter-core/     # Token bucket, consistent hashing (no I/O)
├── limiter-server/   # gRPC server, Redis, Lua scripts
├── limiter-client/  # CLI client
└── proto/            # gRPC definitions
```

## API

- **Allow(key, tokens=1)**: Check if request allowed. Consumes tokens. Returns allowed/denied.
- **SetLimit(key, capacity, refill_rate)**: Configure rate limit for a key.

## User-Focused Design

- **Fairness**: Per-key limits (user, API key, IP) so one bad actor doesn't starve others.
- **Burst tolerance**: Token bucket allows short bursts (e.g. page load with many assets).
- **Low latency**: gRPC + Redis + Lua → sub-ms p99.
- **Horizontal scaling**: Add more server instances; Redis is shared. Consistent hashing ready for Redis Cluster.

## Learning Path

1. **limiter-core**: Pure algorithm, no deps. Read `bucket.rs`, `consistent_hash.rs`.
2. **limiter-server/service.rs**: How gRPC + Redis fit together.
3. **limiter-server/lua.rs**: Why Lua for atomicity.
4. **proto/limiter.proto**: API contract.
