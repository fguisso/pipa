use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// Daily-rotated salt. The salt itself is derived (HMAC) from the long-lived
/// server HMAC key + the current YYYY-MM-DD, so rotation requires no extra
/// storage and survives restarts.
pub struct SaltStore {
    inner: Mutex<(String, [u8; 32])>,
}

impl SaltStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new((String::new(), [0u8; 32])),
        }
    }
}

impl Default for SaltStore {
    fn default() -> Self {
        Self::new()
    }
}

fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs / 86_400;
    let (y, m, d) = unix_day_to_ymd(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Civil-from-days (Howard Hinnant). days is days since 1970-01-01.
fn unix_day_to_ymd(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y_in_era = yoe as i64;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (y_in_era + era * 400 + if m <= 2 { 1 } else { 0 }) as i32;
    (y, m, d)
}

fn salt_for(state: &AppState, day: &str) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"ip-salt/v1/");
    mac.update(day.as_bytes());
    let out = mac.finalize().into_bytes();
    let mut salt = [0u8; 32];
    salt.copy_from_slice(&out);
    salt
}

/// HMAC-SHA256 of the IP using the current day's salt, returned as hex.
pub fn hmac_ip(state: &AppState, ip: &str) -> String {
    let day = today();
    let salt = {
        let mut guard = state.salts.inner.lock().expect("salt mutex");
        if guard.0 != day {
            let s = salt_for(state, &day);
            *guard = (day.clone(), s);
        }
        guard.1
    };

    let mut mac = HmacSha256::new_from_slice(&salt).expect("HMAC accepts any key length");
    mac.update(ip.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// HMAC-SHA256 of an arbitrary value (used for UA hashing) keyed by the
/// long-lived server HMAC key. We do NOT use the daily salt here — the goal
/// is correlation across days for unique counting if we ever want it; for now
/// it just keeps the UA out of plaintext.
pub fn hmac_value(state: &AppState, value: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(state.hmac_key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(b"ua/v1/");
    mac.update(value.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}
