//! Redis Lua scripts for atomic token bucket operations.
//!
//! ## Why Lua and not a Redis transaction (MULTI/EXEC)?
//!
//! MULTI/EXEC with WATCH requires the client to retry on contention — under high load this
//! creates a retry storm. Lua scripts execute atomically on the Redis thread: no other command
//! runs between our GET and SET, so there is no contention at all. One round-trip, no retries.
//!
//! ## Why N-token support in the script?
//!
//! A previous version used Lua only for single-token requests and fell back to a non-atomic
//! read-modify-write for multi-token requests (e.g. batch operations). That fallback had a
//! TOCTOU race: two concurrent batch requests could both read a sufficient token count, both
//! consume, and both succeed — allowing 2× the intended burst. Moving all consumption into the
//! Lua script (ARGV[3] = tokens to consume) closes this hole for free.

/// Atomic allow: get state, refill, consume N tokens if available, save, return 1 or 0.
/// KEYS[1]: redis key
/// ARGV[1]: default capacity
/// ARGV[2]: default refill_rate
/// ARGV[3]: tokens to consume (≥ 1)
/// Returns: 1 if allowed, 0 if rate limited
pub const ALLOW_SCRIPT: &str = r#"
local raw = redis.call('GET', KEYS[1])
local now = redis.call('TIME')
local now_secs = tonumber(now[1])
local now_nanos = tonumber(now[2]) * 1000  -- Redis returns microseconds
local default_cap = tonumber(ARGV[1])
local default_refill = tonumber(ARGV[2])
local consume = tonumber(ARGV[3])

local tokens, last_secs, last_nanos, capacity, refill_rate
if raw then
  local parts = {}
  for p in string.gmatch(raw, "[^:]+") do table.insert(parts, p) end
  tokens = tonumber(parts[1]) or default_cap
  last_secs = tonumber(parts[2]) or 0
  last_nanos = tonumber(parts[3]) or 0
  capacity = tonumber(parts[4]) or default_cap
  refill_rate = tonumber(parts[5]) or default_refill
else
  tokens = default_cap
  last_secs = now_secs
  last_nanos = now_nanos
  capacity = default_cap
  refill_rate = default_refill
end

local elapsed = (now_secs - last_secs) + (now_nanos - last_nanos) / 1e9
if elapsed < 0 then elapsed = 0 end
tokens = math.min(capacity, tokens + elapsed * refill_rate)

if tokens >= consume then
  tokens = tokens - consume
  redis.call('SETEX', KEYS[1], 86400, string.format("%.6f:%d:%d:%d:%.6f", tokens, now_secs, now_nanos, capacity, refill_rate))
  return 1
else
  redis.call('SETEX', KEYS[1], 86400, string.format("%.6f:%d:%d:%d:%.6f", tokens, now_secs, now_nanos, capacity, refill_rate))
  return 0
end
"#;
