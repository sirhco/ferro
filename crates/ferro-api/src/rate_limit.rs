//! Per-IP token-bucket rate limiter.
//!
//! Hand-rolled to avoid pulling in a heavier crate. Sized for the auth
//! endpoints — login + signup — where stuffing/credential-spray is the
//! primary concern. Buckets live in memory; replace with Redis when the
//! deployment is multi-process.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct RateLimitConfig {
    pub max_burst: u32,
    pub refill_per_sec: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        // 10-request burst, refill 1 every 6s ⇒ ~10/min sustained.
        Self { max_burst: 10, refill_per_sec: 1.0 / 6.0 }
    }
}

#[derive(Debug)]
pub struct RateLimiter {
    cfg: RateLimitConfig,
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    #[must_use]
    pub fn new(cfg: RateLimitConfig) -> Self {
        Self { cfg, buckets: Mutex::new(HashMap::new()) }
    }

    /// Try to claim one token. Returns `Some(retry_after)` if rate-limited.
    pub fn check(&self, ip: IpAddr) -> Option<Duration> {
        let mut buckets = self.buckets.lock().expect("rate-limit mutex poisoned");
        let now = Instant::now();
        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.cfg.max_burst as f64,
            last: now,
        });
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.cfg.refill_per_sec)
            .min(self.cfg.max_burst as f64);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            None
        } else {
            // Avoid div-by-zero when refill_per_sec is 0 (a hard cap with no
            // recovery); report a 1-hour retry window in that case.
            let retry = if self.cfg.refill_per_sec <= f64::EPSILON {
                Duration::from_secs(3600)
            } else {
                let needed = 1.0 - bucket.tokens;
                Duration::from_secs_f64(needed / self.cfg.refill_per_sec)
            };
            Some(retry)
        }
    }

    /// Drop buckets idle for longer than `ttl`. Caller spawns a background
    /// task on a `tokio::time::interval` if desired.
    pub fn purge_idle(&self, ttl: Duration) {
        let cutoff = Instant::now().checked_sub(ttl);
        let Some(cutoff) = cutoff else { return };
        let mut buckets = self.buckets.lock().expect("rate-limit mutex poisoned");
        buckets.retain(|_, b| b.last >= cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn burst_then_refill() {
        let rl = RateLimiter::new(RateLimitConfig { max_burst: 3, refill_per_sec: 1000.0 });
        let ip = IpAddr::V4(Ipv4Addr::LOCALHOST);
        // Initial 3 succeed
        for _ in 0..3 {
            assert!(rl.check(ip).is_none());
        }
        // 4th rate-limited
        assert!(rl.check(ip).is_some());
        // Wait long enough for one token (1ms at 1000/s).
        std::thread::sleep(Duration::from_millis(2));
        assert!(rl.check(ip).is_none(), "should refill");
    }

    #[test]
    fn isolates_ips() {
        let rl = RateLimiter::new(RateLimitConfig { max_burst: 1, refill_per_sec: 0.0 });
        let a = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));
        let b = IpAddr::V4(Ipv4Addr::new(2, 2, 2, 2));
        assert!(rl.check(a).is_none());
        assert!(rl.check(a).is_some());
        // Different IP gets its own bucket.
        assert!(rl.check(b).is_none());
    }
}
