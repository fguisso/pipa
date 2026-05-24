//! Tiny in-memory sliding-window rate limiter used for the comments POST.
//!
//! Per `SECURITY.md` §4 we cap public comment submissions at 10/min per
//! (ip_hash, page_uuid) and 100/hour per ip_hash globally. The limiter is
//! deliberately approximate: bounded mutex sections, no background sweep —
//! entries are trimmed lazily on each `check` call. Phase 1 single-process,
//! single-binary; nothing fancier is warranted.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const PER_PAGE_LIMIT: usize = 10;
const PER_PAGE_WINDOW_SECS: i64 = 60;

const PER_SERVER_LIMIT: usize = 100;
const PER_SERVER_WINDOW_SECS: i64 = 3_600;

#[derive(Debug, PartialEq, Eq)]
pub enum RateLimitResult {
    Ok,
    Retry { after_secs: u64 },
}

#[derive(Default)]
pub struct CommentLimiter {
    per_ip_page: Mutex<HashMap<(String, String), VecDeque<i64>>>,
    per_ip_server: Mutex<HashMap<String, VecDeque<i64>>>,
}

impl CommentLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check + record a hit. We only record when allowed; rejected attempts
    /// don't count toward the window (keeps the cap from being self-stretched
    /// by abusive callers).
    pub fn check(&self, ip_hash: &str, page_uuid: &str, now: i64) -> RateLimitResult {
        if let Some(after) = self.check_per_ip_server(ip_hash, now) {
            return RateLimitResult::Retry { after_secs: after };
        }
        if let Some(after) = self.check_per_ip_page(ip_hash, page_uuid, now) {
            return RateLimitResult::Retry { after_secs: after };
        }

        self.record_per_ip_server(ip_hash, now);
        self.record_per_ip_page(ip_hash, page_uuid, now);
        RateLimitResult::Ok
    }

    fn check_per_ip_page(&self, ip_hash: &str, page_uuid: &str, now: i64) -> Option<u64> {
        let mut guard = self.per_ip_page.lock().expect("per_ip_page mutex");
        let key = (ip_hash.to_string(), page_uuid.to_string());
        let entries = guard.entry(key).or_default();
        trim(entries, now, PER_PAGE_WINDOW_SECS);
        if entries.len() >= PER_PAGE_LIMIT {
            let oldest = *entries.front().unwrap_or(&now);
            return Some(retry_secs(oldest, now, PER_PAGE_WINDOW_SECS));
        }
        None
    }

    fn check_per_ip_server(&self, ip_hash: &str, now: i64) -> Option<u64> {
        let mut guard = self.per_ip_server.lock().expect("per_ip_server mutex");
        let entries = guard.entry(ip_hash.to_string()).or_default();
        trim(entries, now, PER_SERVER_WINDOW_SECS);
        if entries.len() >= PER_SERVER_LIMIT {
            let oldest = *entries.front().unwrap_or(&now);
            return Some(retry_secs(oldest, now, PER_SERVER_WINDOW_SECS));
        }
        None
    }

    fn record_per_ip_page(&self, ip_hash: &str, page_uuid: &str, now: i64) {
        let mut guard = self.per_ip_page.lock().expect("per_ip_page mutex");
        let key = (ip_hash.to_string(), page_uuid.to_string());
        guard.entry(key).or_default().push_back(now);
    }

    fn record_per_ip_server(&self, ip_hash: &str, now: i64) {
        let mut guard = self.per_ip_server.lock().expect("per_ip_server mutex");
        guard.entry(ip_hash.to_string()).or_default().push_back(now);
    }
}

fn trim(entries: &mut VecDeque<i64>, now: i64, window: i64) {
    let cutoff = now - window;
    while let Some(&front) = entries.front() {
        if front < cutoff {
            entries.pop_front();
        } else {
            break;
        }
    }
}

fn retry_secs(oldest: i64, now: i64, window: i64) -> u64 {
    let after = (oldest + window) - now;
    after.max(1) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_under_the_per_page_cap() {
        let l = CommentLimiter::new();
        for i in 0..PER_PAGE_LIMIT {
            assert_eq!(l.check("ip", "page", 1000 + i as i64), RateLimitResult::Ok);
        }
        let r = l.check("ip", "page", 1010);
        assert!(matches!(r, RateLimitResult::Retry { .. }));
    }

    #[test]
    fn window_expires() {
        let l = CommentLimiter::new();
        for i in 0..PER_PAGE_LIMIT {
            l.check("ip", "page", 1000 + i as i64);
        }
        assert!(matches!(
            l.check("ip", "page", 1010),
            RateLimitResult::Retry { .. }
        ));
        // > window later: old entries are trimmed.
        assert_eq!(l.check("ip", "page", 1000 + PER_PAGE_WINDOW_SECS + 1), RateLimitResult::Ok);
    }
}
