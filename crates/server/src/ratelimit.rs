use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Token bucket. `capacity` tokens, refilled continuously at `per_sec`.
#[derive(Debug, Clone)]
pub struct Bucket {
    tokens: f64,
    last: Instant,
    capacity: f64,
    per_sec: f64,
}

impl Bucket {
    pub fn new(capacity: f64, per_sec: f64) -> Self {
        Bucket { tokens: capacity, last: Instant::now(), capacity, per_sec }
    }

    pub fn take_at(&mut self, now: Instant, cost: f64) -> bool {
        let dt = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + dt * self.per_sec).min(self.capacity);
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }

    pub fn take(&mut self) -> bool {
        self.take_at(Instant::now(), 1.0)
    }
}

/// Keyed buckets with periodic eviction of idle keys.
pub struct Limiter {
    capacity: f64,
    per_sec: f64,
    inner: Mutex<(HashMap<String, Bucket>, Instant)>,
}

impl Limiter {
    pub fn new(capacity: f64, per_sec: f64) -> Self {
        Limiter { capacity, per_sec, inner: Mutex::new((HashMap::new(), Instant::now())) }
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut guard = self.inner.lock().unwrap();
        let (map, last_sweep) = &mut *guard;
        if now.duration_since(*last_sweep) > Duration::from_secs(60) {
            let full = self.capacity / self.per_sec;
            map.retain(|_, b| now.duration_since(b.last).as_secs_f64() < full);
            *last_sweep = now;
        }
        map.entry(key.to_string()).or_insert_with(|| Bucket::new(self.capacity, self.per_sec)).take_at(now, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_refills_over_time() {
        let mut b = Bucket::new(2.0, 1.0);
        let t0 = Instant::now();
        assert!(b.take_at(t0, 1.0));
        assert!(b.take_at(t0, 1.0));
        assert!(!b.take_at(t0, 1.0));
        assert!(b.take_at(t0 + Duration::from_millis(1100), 1.0));
    }

    #[test]
    fn limiter_is_per_key() {
        let l = Limiter::new(1.0, 0.001);
        assert!(l.check("a"));
        assert!(!l.check("a"));
        assert!(l.check("b"));
    }
}
