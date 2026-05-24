//! Shared helpers for the adapter integration tests.
//!
//! - `FakeClock` lets a test advance time deterministically (so we can prove
//!   expiry-driven branches without `tokio::time::sleep`).
//! - `FakeIdGen` returns monotonically increasing ULIDs derived from a counter
//!   so tests can assert on the IDs they expect.
//! - `setup_in_memory_db` opens a fresh in-memory SQLite, runs migrations, and
//!   hands back the pool.

use std::sync::{Arc, Mutex};

use pipa_adapters::{open_pool, run_migrations};
use pipa_core::ids::IdGen;
use pipa_core::time::Clock;
use sqlx::SqlitePool;
use ulid::Ulid;

/// Atomic-int-backed clock; tests start at `t0` and advance via `set` / `add`.
///
/// Each integration test binary compiles `common/mod.rs` separately, so we
/// `#[allow(dead_code)]` on each helper rather than gating per-binary.
#[allow(dead_code)]
pub struct FakeClock(Arc<Mutex<i64>>);

#[allow(dead_code)]
impl FakeClock {
    pub fn new(t0: i64) -> Self {
        Self(Arc::new(Mutex::new(t0)))
    }

    pub fn set(&self, ts: i64) {
        *self.0.lock().expect("FakeClock mutex") = ts;
    }

    pub fn add(&self, delta: i64) {
        let mut g = self.0.lock().expect("FakeClock mutex");
        *g += delta;
    }

    pub fn arc(t0: i64) -> Arc<FakeClock> {
        Arc::new(Self::new(t0))
    }
}

impl Clock for FakeClock {
    fn now(&self) -> i64 {
        *self.0.lock().expect("FakeClock mutex")
    }
}

/// ULID generator backed by an i64 counter. The high 80 bits are zero, the
/// low 48 bits encode the counter — deterministic and unique for a single
/// test run. Real ULIDs are 128 bits; this is wire-compatible.
#[allow(dead_code)]
pub struct FakeIdGen(Arc<Mutex<u64>>);

#[allow(dead_code)]
impl FakeIdGen {
    pub fn new(start: u64) -> Self {
        Self(Arc::new(Mutex::new(start)))
    }

    pub fn arc(start: u64) -> Arc<FakeIdGen> {
        Arc::new(Self::new(start))
    }
}

impl IdGen for FakeIdGen {
    fn new_ulid(&self) -> Ulid {
        let mut g = self.0.lock().expect("FakeIdGen mutex");
        let v = *g;
        *g += 1;
        // 128-bit ULID with the counter packed in the low bits. We don't need
        // monotonicity vs wall-clock here.
        Ulid::from_parts(0, v as u128)
    }
}

#[allow(dead_code)]
pub async fn setup_in_memory_db() -> SqlitePool {
    let pool = open_pool("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    run_migrations(&pool).await.expect("migrate in-memory sqlite");
    pool
}
