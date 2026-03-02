//! Redis Lua scripts for atomic token bucket operations.
//!
//! Without Lua, concurrent requests could race: two requests read 1 token,
//! both consume, both write — we'd incorrectly allow both. Lua runs atomically.

/// Atomic allow: get state, refill, consume 1 token if available, save, return 1 or 0.
/// KEYS[1]: redis key
/// ARGV[1]: default capacity
/// ARGV[2]: default refill_rate
/// Returns: 1 if allowed, 0 if rate limited
pub const ALLOW_SCRIPT: &str = r#"
local raw = redis.call('GET', KEYS[1])
local now = redis.call('TIME')
local now_secs = tonumber(now[1])
local now_nanos = tonumber(now[2]) * 1000  -- Redis returns microseconds
local default_cap = tonumber(ARGV[1])
local default_refill = tonumber(ARGV[2])

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

if tokens >= 1 then
  tokens = tokens - 1
  redis.call('SETEX', KEYS[1], 86400, string.format("%.6f:%d:%d:%d:%.6f", tokens, now_secs, now_nanos, capacity, refill_rate))
  return 1
else
  redis.call('SETEX', KEYS[1], 86400, string.format("%.6f:%d:%d:%d:%.6f", tokens, now_secs, now_nanos, capacity, refill_rate))
  return 0
end
"#;
