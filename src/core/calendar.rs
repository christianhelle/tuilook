//! Keeping the cached calendar in step with the server's.

use jiff::{SignedDuration, Timestamp};

use crate::model::MailBackend;
use crate::store::Store;

/// How far back the cached calendar reaches.
const DAYS_BACK: i64 = 7;

/// How far ahead the cached calendar reaches.
const DAYS_AHEAD: i64 = 90;

/// The stretch of time around `now` whose appointments are cached.
pub fn calendar_window(now: Timestamp) -> (Timestamp, Timestamp) {
    (
        now - SignedDuration::from_hours(DAYS_BACK * 24),
        now + SignedDuration::from_hours(DAYS_AHEAD * 24),
    )
}

/// Caches the server's appointments around today, in place of those cached
/// there before.
pub(crate) async fn sync_calendar(
    backend: &impl MailBackend,
    store: &mut Store,
) -> Result<(), String> {
    let (from, to) = calendar_window(Timestamp::now());
    let appointments = backend
        .appointments(from, to)
        .await
        .map_err(|e| e.to_string())?;
    store
        .replace_appointments(from, to, &appointments)
        .map_err(|e| e.to_string())
}
