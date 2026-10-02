//Bearer check against of-api's /api/v1/me with a positive cache keyed by the token's sha256.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::state::AppState;

const TIMEOUT: Duration = Duration::from_secs(2);

type Key = [u8; 32];

pub enum AuthError {
    Invalid,
    Unavailable,
}

pub struct TokenCache {
    ttl: Duration,
    entries: Mutex<HashMap<Key, Instant>>,
}

#[derive(Deserialize)]
struct Me {
    expires_at: String,
}

pub async fn check(state: &AppState, headers: &HeaderMap) -> Result<(), AuthError> {
    let token = bearer(headers).ok_or(AuthError::Invalid)?;
    let key: Key = Sha256::digest(token.as_bytes()).into();
    if state.cache.contains(&key) {
        return Ok(());
    }
    let response = state
        .client
        .get(state.me_url.clone())
        .bearer_auth(token)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|_| AuthError::Unavailable)?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::UNAUTHORIZED => return Err(AuthError::Invalid),
        _ => return Err(AuthError::Unavailable),
    }
    let me: Me = response.json().await.map_err(|_| AuthError::Unavailable)?;
    let expires_at = unix_seconds(&me.expires_at).ok_or(AuthError::Unavailable)?;
    state.cache.insert(key, until(expires_at));
    Ok(())
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(AUTHORIZATION)?.to_str().ok()?.split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

fn until(expires_at: u64) -> Duration {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    Duration::from_secs(expires_at).saturating_sub(now)
}

impl TokenCache {
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn contains(&self, key: &Key) -> bool {
        self.lock()
            .get(key)
            .is_some_and(|deadline| *deadline > Instant::now())
    }

    fn insert(&self, key: Key, valid_for: Duration) {
        let ttl = self.ttl.min(valid_for);
        if ttl.is_zero() {
            return;
        }
        let now = Instant::now();
        let mut entries = self.lock();
        entries.retain(|_, deadline| *deadline > now);
        entries.insert(key, now + ttl);
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Instant>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

//RFC 3339 timestamp to Unix seconds, fraction dropped.
fn unix_seconds(timestamp: &str) -> Option<u64> {
    let bytes = timestamp.as_bytes();
    let separators = [(4, b'-'), (7, b'-'), (13, b':'), (16, b':')];
    if !separators
        .iter()
        .all(|&(at, byte)| bytes.get(at) == Some(&byte))
        || !matches!(bytes.get(10), Some(b'T' | b't' | b' '))
    {
        return None;
    }
    let field = |from: usize, to: usize| digits(timestamp.get(from..to));
    let (year, month, day) = (field(0, 4)?, field(5, 7)?, field(8, 10)?);
    let (hour, minute, second) = (field(11, 13)?, field(14, 16)?, field(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let mut zone = timestamp.get(19..)?;
    if let Some(fraction) = zone.strip_prefix('.') {
        let count = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if count == 0 {
            return None;
        }
        zone = &fraction[count..];
    }
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let sign = match zone.as_bytes().first() {
                Some(b'+') => 1,
                Some(b'-') => -1,
                _ => return None,
            };
            if zone.len() != 6 || zone.as_bytes()[3] != b':' {
                return None;
            }
            sign * (digits(zone.get(1..3))? * 3600 + digits(zone.get(4..6))? * 60)
        }
    };
    let days = days_from_civil(year, month, day);
    u64::try_from(days * 86_400 + hour * 3600 + minute * 60 + second - offset).ok()
}

fn digits(part: Option<&str>) -> Option<i64> {
    part.filter(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))?
        .parse()
        .ok()
}

//Days since 1970-01-01 in the proleptic Gregorian calendar (Hinnant, days_from_civil).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}
