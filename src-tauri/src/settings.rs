//! Setting keys the UI may read and write, and typed helpers.

use crate::db::Db;

pub const CONSENT_REMINDER: &str = "recording.consentReminder";
pub const TRANSCRIBE_MODEL: &str = "transcription.model";
pub const TRANSCRIBE_LANGUAGE: &str = "transcription.language";
pub const TRANSCRIBE_PAUSED: &str = "transcription.paused";
pub const TRACKING_ENABLED: &str = "tracking.enabled";
pub const TRACKING_IDLE_MINUTES: &str = "tracking.idleMinutes";
pub const TRACKING_BLOCKLIST: &str = "tracking.blocklist";
pub const TRACKING_BROWSER_URLS: &str = "tracking.browserUrls";
pub const MEETING_DETECTION: &str = "meetings.detection";
pub const MEETING_KEYWORDS: &str = "meetings.keywords";
pub const NOTES_PROVIDER: &str = "notes.provider";
pub const NOTES_MODEL: &str = "notes.model";
pub const NOTES_BASE_URL: &str = "notes.baseUrl";
pub const NOTES_AUTO: &str = "notes.auto";
pub const STORAGE_RECORDINGS_DIR: &str = "storage.recordingsDir";
pub const STORAGE_DELETE_AUDIO_DAYS: &str = "storage.deleteAudioAfterDays";
pub const ONBOARDING_DONE: &str = "onboarding.done";

/// Only these keys can be read or written from the UI.
pub const ALLOWED: &[&str] = &[
    CONSENT_REMINDER,
    TRANSCRIBE_MODEL,
    TRANSCRIBE_LANGUAGE,
    TRANSCRIBE_PAUSED,
    TRACKING_ENABLED,
    TRACKING_IDLE_MINUTES,
    TRACKING_BLOCKLIST,
    TRACKING_BROWSER_URLS,
    MEETING_DETECTION,
    MEETING_KEYWORDS,
    NOTES_PROVIDER,
    NOTES_MODEL,
    NOTES_BASE_URL,
    NOTES_AUTO,
    STORAGE_RECORDINGS_DIR,
    STORAGE_DELETE_AUDIO_DAYS,
    ONBOARDING_DONE,
];

pub fn get_bool(db: &Db, key: &str, default: bool) -> bool {
    match db.get_setting(key) {
        Ok(Some(serde_json::Value::Bool(v))) => v,
        Ok(_) => default,
        Err(err) => {
            tracing::warn!(%err, key, "could not read setting");
            default
        }
    }
}

pub fn get_string(db: &Db, key: &str) -> Option<String> {
    match db.get_setting(key) {
        Ok(Some(serde_json::Value::String(v))) if !v.is_empty() => Some(v),
        Ok(_) => None,
        Err(err) => {
            tracing::warn!(%err, key, "could not read setting");
            None
        }
    }
}

pub fn get_u64(db: &Db, key: &str, default: u64) -> u64 {
    match db.get_setting(key) {
        Ok(Some(v)) => v.as_u64().unwrap_or(default),
        _ => default,
    }
}

pub fn get_string_list(db: &Db, key: &str) -> Vec<String> {
    match db.get_setting(key) {
        Ok(Some(serde_json::Value::Array(items))) => items
            .into_iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}
