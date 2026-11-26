//! AI meeting notes: summary, decisions and action items from a transcript.
//!
//! Off until the user picks a provider (principle 2). Providers:
//! - `ollama`: local model through Ollama (nothing leaves the machine)
//! - `openai`: any OpenAI-compatible server (LM Studio, vLLM, OpenAI, ...)
//! - `anthropic`: Claude through the Messages API
//!
//! Prompts contain the transcript text only, never file paths or other meetings.
//! API keys live in the OS keychain, never in SQLite or settings.

mod providers;

use serde::{Deserialize, Serialize};

use crate::db::SegmentRow;
use crate::settings;

pub use providers::{Provider, ProviderKind};

#[derive(Debug, thiserror::Error)]
pub enum NotesError {
    #[error("AI notes are off. Choose a provider in Settings → AI meeting notes.")]
    Disabled,
    #[error("No API key saved. Add it in Settings → AI meeting notes.")]
    MissingKey,
    #[error("request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Provider(String),
    #[error("the model did not return valid notes: {0}")]
    BadOutput(String),
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("this meeting has no transcript yet")]
    NoTranscript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub text: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub due: Option<String>,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    pub summary: String,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub action_items: Vec<ActionItem>,
}

/// JSON schema shared by all providers (snake_case on the wire for the model).
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "decisions", "action_items"],
        "properties": {
            "summary": { "type": "string", "description": "3 to 8 short bullet points, each starting with '- '" },
            "decisions": { "type": "array", "items": { "type": "string" } },
            "action_items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["text", "owner", "due"],
                    "properties": {
                        "text": { "type": "string" },
                        "owner": { "type": ["string", "null"], "description": "Person responsible if stated, else null" },
                        "due": { "type": ["string", "null"], "description": "Deadline as said in the meeting, else null" }
                    }
                }
            }
        }
    })
}

pub const SYSTEM_PROMPT: &str = "You write meeting notes from a transcript. Lines are labelled \
\"You\" (the person who recorded the meeting), \"Others\" (other participants) or \"Unknown\". \
The transcript comes from automatic speech recognition and may contain mistakes; infer the intended \
meaning but do not invent facts. Write in the language of the meeting. Keep the summary short and \
concrete. List only decisions that were actually made. An action item is a task someone agreed to \
do; set owner to \"You\" when the recorder took it on, a name if one was said, otherwise null. \
Respond with JSON only, matching the requested schema.";

/// Wire format from the model (snake_case), converted to `Notes`.
#[derive(Deserialize)]
struct WireNotes {
    summary: String,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    action_items: Vec<WireItem>,
}

#[derive(Deserialize)]
struct WireItem {
    text: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    due: Option<String>,
}

/// Parse model output. Tolerates code fences or text around the JSON object.
pub fn parse_notes(raw: &str) -> Result<Notes, NotesError> {
    let start = raw.find('{');
    let end = raw.rfind('}');
    let json = match (start, end) {
        (Some(s), Some(e)) if e > s => &raw[s..=e],
        _ => return Err(NotesError::BadOutput("no JSON object in reply".into())),
    };
    let wire: WireNotes =
        serde_json::from_str(json).map_err(|e| NotesError::BadOutput(e.to_string()))?;
    Ok(Notes {
        summary: wire.summary.trim().to_string(),
        decisions: wire
            .decisions
            .into_iter()
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .collect(),
        action_items: wire
            .action_items
            .into_iter()
            .filter(|i| !i.text.trim().is_empty())
            .map(|i| ActionItem {
                text: i.text.trim().to_string(),
                owner: i.owner.filter(|o| !o.trim().is_empty()),
                due: i.due.filter(|d| !d.trim().is_empty()),
                done: false,
            })
            .collect(),
    })
}

fn clock(ms: i64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Transcript as plain text lines: `[00:01:23] Others: text`.
pub fn transcript_text(segments: &[SegmentRow]) -> String {
    segments
        .iter()
        .map(|s| {
            let who = match s.speaker.as_str() {
                "you" => "You",
                "others" => "Others",
                _ => "Unknown",
            };
            format!("[{}] {who}: {}\n", clock(s.start_ms), s.text)
        })
        .collect()
}

/// Split long transcripts on line boundaries so small local models are not overflowed.
pub fn chunks(text: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if !cur.is_empty() && cur.len() + line.len() + 1 > max_chars {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// Generate notes. Long transcripts are summarized part by part, then the partial notes merged.
pub async fn generate(provider: &Provider, segments: &[SegmentRow]) -> Result<Notes, NotesError> {
    if segments.is_empty() {
        return Err(NotesError::NoTranscript);
    }
    let text = transcript_text(segments);
    let parts = chunks(&text, provider.max_chars());
    if parts.len() == 1 {
        let raw = provider
            .complete(SYSTEM_PROMPT, &format!("Transcript:\n\n{text}"))
            .await?;
        return parse_notes(&raw);
    }
    let mut partial = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let prompt = format!(
            "This is part {} of {} of a long meeting transcript.\n\n{part}",
            i + 1,
            parts.len()
        );
        partial.push(parse_notes(
            &provider.complete(SYSTEM_PROMPT, &prompt).await?,
        )?);
    }
    let merged = serde_json::to_string(&partial).unwrap_or_default();
    let raw = provider
        .complete(
            SYSTEM_PROMPT,
            &format!(
                "These are notes for consecutive parts of one meeting. Merge them into one set of notes for the \
                 whole meeting: remove duplicates, keep every distinct decision and action item.\n\n{merged}"
            ),
        )
        .await?;
    parse_notes(&raw)
}

/// Read the provider configuration from settings and the keychain.
pub fn configured_provider(db: &crate::db::Db) -> Result<Provider, NotesError> {
    let kind = ProviderKind::parse(settings::get_string(db, settings::NOTES_PROVIDER).as_deref())
        .ok_or(NotesError::Disabled)?;
    let model = settings::get_string(db, settings::NOTES_MODEL);
    let base_url = settings::get_string(db, settings::NOTES_BASE_URL);
    let key = if kind.needs_key() {
        keychain_get(kind)?
    } else {
        None
    };
    Ok(Provider::new(kind, model, base_url, key))
}

const KEYCHAIN_SERVICE: &str = "io.github.rishapgandhi.taskmind";

fn entry(kind: ProviderKind) -> Result<keyring::Entry, NotesError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("notes-{}", kind.id()))
        .map_err(|e| NotesError::Keychain(e.to_string()))
}

pub fn keychain_get(kind: ProviderKind) -> Result<Option<String>, NotesError> {
    match entry(kind)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(NotesError::Keychain(e.to_string())),
    }
}

pub fn keychain_set(kind: ProviderKind, key: Option<&str>) -> Result<(), NotesError> {
    let e = entry(kind)?;
    match key.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => e
            .set_password(k)
            .map_err(|e| NotesError::Keychain(e.to_string())),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(NotesError::Keychain(e.to_string())),
        },
    }
}

pub const CHANGED_EVENT: &str = "notes://changed";

/// Generate and store notes for one meeting. The outcome (or the reason it failed) is saved
/// on the meeting so the list and meeting page can show it later.
pub async fn generate_for_meeting(
    app: &tauri::AppHandle,
    meeting_id: i64,
) -> Result<Notes, NotesError> {
    use tauri::{Emitter, Manager};
    let db = app.state::<crate::db::Db>();
    let _ = app.emit(
        CHANGED_EVENT,
        serde_json::json!({ "meetingId": meeting_id, "state": "working" }),
    );
    let result = async {
        let provider = configured_provider(&db)?;
        let segments = db
            .segments(meeting_id)
            .map_err(|e| NotesError::Provider(e.to_string()))?;
        let notes = generate(&provider, &segments).await?;
        let (kind, model) = provider.label();
        let at = chrono::Utc::now().to_rfc3339();
        db.save_notes(meeting_id, &notes, Some((kind, model, &at)))
            .map_err(|e| NotesError::Provider(e.to_string()))?;
        Ok(notes)
    }
    .await;
    let error = result.as_ref().err().map(ToString::to_string);
    if let Err(err) = db.set_notes_error(meeting_id, error.as_deref()) {
        tracing::warn!(%err, meeting_id, "could not store notes status");
    }
    match &error {
        None => {
            let _ = app.emit(
                CHANGED_EVENT,
                serde_json::json!({ "meetingId": meeting_id, "state": "done" }),
            );
        }
        Some(err) => {
            tracing::warn!(%err, meeting_id, "notes generation failed");
            let _ = app.emit(
                CHANGED_EVENT,
                serde_json::json!({ "meetingId": meeting_id, "state": "failed", "error": err }),
            );
        }
    }
    let _ = app.emit("meetings://changed", meeting_id);
    result
}

/// Check that the configured provider answers (no tokens are spent).
pub async fn check_provider(db: &crate::db::Db) -> Result<String, NotesError> {
    let provider = configured_provider(db)?;
    provider.check().await
}

/// After a transcript is saved: generate notes in the background if the user turned that on.
pub fn maybe_auto_generate(app: &tauri::AppHandle, meeting_id: i64) {
    use tauri::Manager;
    let db = app.state::<crate::db::Db>();
    if !settings::get_bool(&db, settings::NOTES_AUTO, true) || configured_provider(&db).is_err() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = generate_for_meeting(&app, meeting_id).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fenced_json_and_cleans_items() {
        let raw = "```json\n{\"summary\": \"- Shipped\", \"decisions\": [\"Go Friday\", \" \"],\n\"action_items\": [{\"text\": \"Send invoice\", \"owner\": \"You\", \"due\": null}, {\"text\": \" \", \"owner\": null, \"due\": null}]}\n```";
        let n = parse_notes(raw).unwrap();
        assert_eq!(n.summary, "- Shipped");
        assert_eq!(n.decisions, vec!["Go Friday"]);
        assert_eq!(n.action_items.len(), 1);
        assert_eq!(n.action_items[0].owner.as_deref(), Some("You"));
        assert!(parse_notes("sorry, I can't").is_err());
    }

    #[test]
    fn transcript_and_chunks() {
        let segs = vec![
            SegmentRow {
                start_ms: 83_000,
                end_ms: 85_000,
                speaker: "others".into(),
                text: "Hello".into(),
            },
            SegmentRow {
                start_ms: 90_000,
                end_ms: 91_000,
                speaker: "you".into(),
                text: "Hi".into(),
            },
        ];
        let t = transcript_text(&segs);
        assert_eq!(t, "[00:01:23] Others: Hello\n[00:01:30] You: Hi\n");
        let parts = chunks(&t.repeat(10), 60);
        assert!(parts.len() > 1);
        assert!(
            parts
                .iter()
                .all(|p| p.len() <= 60 || p.lines().count() == 1)
        );
        assert_eq!(parts.concat(), t.repeat(10));
    }

    #[test]
    fn schema_is_strict() {
        let s = schema();
        assert_eq!(s["additionalProperties"], false);
        assert_eq!(
            s["properties"]["action_items"]["items"]["additionalProperties"],
            false
        );
    }
}
