//! HTTP clients for each notes provider. Raw HTTP because there is no official Rust SDK.

use std::time::Duration;

use serde_json::{Value, json};

use super::{NotesError, schema};

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2026-10-21";
const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";
/// On a safety-classifier decline, the API re-runs the request on a fallback model it picks.
const ANTHROPIC_FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OLLAMA_DEFAULT_URL: &str = "http://localhost:11434";
const OLLAMA_DEFAULT_MODEL: &str = "llama3.2";
const OPENAI_DEFAULT_URL: &str = "https://api.openai.com/v1";
const OPENAI_DEFAULT_MODEL: &str = "gpt-4o-mini";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Ollama,
    OpenAi,
    Anthropic,
}

impl ProviderKind {
    pub fn parse(s: Option<&str>) -> Option<Self> {
        match s? {
            "ollama" => Some(Self::Ollama),
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }

    pub fn needs_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }
}

pub struct Provider {
    kind: ProviderKind,
    model: String,
    base_url: String,
    key: Option<String>,
    http: reqwest::Client,
}

impl Provider {
    pub fn new(
        kind: ProviderKind,
        model: Option<String>,
        base_url: Option<String>,
        key: Option<String>,
    ) -> Self {
        let (default_model, default_url) = match kind {
            ProviderKind::Ollama => (OLLAMA_DEFAULT_MODEL, OLLAMA_DEFAULT_URL),
            ProviderKind::OpenAi => (OPENAI_DEFAULT_MODEL, OPENAI_DEFAULT_URL),
            ProviderKind::Anthropic => (ANTHROPIC_DEFAULT_MODEL, ""),
        };
        let http = reqwest::Client::builder()
            // Local models on a laptop CPU can take minutes for a long meeting.
            .timeout(Duration::from_secs(600))
            .build()
            .unwrap_or_default();
        Self {
            kind,
            model: model.unwrap_or_else(|| default_model.to_string()),
            base_url: base_url
                .unwrap_or_else(|| default_url.to_string())
                .trim_end_matches('/')
                .to_string(),
            key,
            http,
        }
    }

    pub fn label(&self) -> (&'static str, &str) {
        (self.kind.id(), &self.model)
    }

    /// Transcript characters per request. Small local models get small chunks.
    pub fn max_chars(&self) -> usize {
        match self.kind {
            ProviderKind::Ollama => 24_000,
            ProviderKind::OpenAi => 200_000,
            ProviderKind::Anthropic => 1_500_000,
        }
    }

    /// Cheap connectivity check: Ollama must be running and have the model pulled;
    /// cloud providers need a saved key (checked without sending a billed request).
    pub async fn check(&self) -> Result<String, NotesError> {
        match self.kind {
            ProviderKind::Ollama => {
                let res = self
                    .http
                    .get(format!("{}/api/tags", self.base_url))
                    .timeout(Duration::from_secs(5))
                    .send()
                    .await
                    .map_err(|_| {
                        NotesError::Provider(format!(
                            "Ollama is not running at {}. Install it from ollama.com, start it, then run: ollama pull {}",
                            self.base_url, self.model
                        ))
                    })?;
                let v = checked(res).await?;
                let installed: Vec<String> = v
                    .get("models")
                    .and_then(Value::as_array)
                    .map(|m| {
                        m.iter()
                            .filter_map(|x| x.get("name").and_then(Value::as_str).map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let wanted = if self.model.contains(':') {
                    self.model.clone()
                } else {
                    format!("{}:latest", self.model)
                };
                if installed.iter().any(|m| *m == wanted || *m == self.model) {
                    Ok(format!("Ollama is running and has {}", self.model))
                } else {
                    Err(NotesError::Provider(format!(
                        "Ollama is running but the model is missing. Run: ollama pull {}",
                        self.model
                    )))
                }
            }
            ProviderKind::OpenAi | ProviderKind::Anthropic => {
                if self.key.is_none() {
                    return Err(NotesError::MissingKey);
                }
                Ok(format!("API key saved. Notes will use {}", self.model))
            }
        }
    }

    pub async fn complete(&self, system: &str, user: &str) -> Result<String, NotesError> {
        match self.kind {
            ProviderKind::Ollama => self.ollama(system, user).await,
            ProviderKind::OpenAi => self.openai(system, user).await,
            ProviderKind::Anthropic => self.anthropic(system, user).await,
        }
    }

    async fn ollama(&self, system: &str, user: &str) -> Result<String, NotesError> {
        let body = json!({
            "model": self.model,
            "stream": false,
            "format": schema(),
            "options": { "num_ctx": 16384, "temperature": 0.2 },
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });
        let res = self
            .http
            .post(format!("{}/api/chat", self.base_url))
            .json(&body)
            .send()
            .await;
        let res = res.map_err(|e| {
            if e.is_connect() {
                NotesError::Provider(format!(
                    "Ollama is not running at {}. Install it from ollama.com, start it, then run: ollama pull {}",
                    self.base_url, self.model
                ))
            } else {
                NotesError::Http(e)
            }
        })?;
        let v = checked(res).await?;
        text_at(&v, &["message", "content"])
    }

    async fn openai(&self, system: &str, user: &str) -> Result<String, NotesError> {
        let key = self.key.as_deref().ok_or(NotesError::MissingKey)?;
        let body = json!({
            "model": self.model,
            "temperature": 0.2,
            // json_object is the most widely supported JSON mode; the schema is in the prompt.
            "response_format": { "type": "json_object" },
            "messages": [
                { "role": "system", "content": format!("{system}\n\nJSON schema:\n{}", schema()) },
                { "role": "user", "content": user },
            ],
        });
        let res = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(key)
            .json(&body)
            .send()
            .await?;
        let v = checked(res).await?;
        text_at(&v, &["choices", "0", "message", "content"])
    }

    async fn anthropic(&self, system: &str, user: &str) -> Result<String, NotesError> {
        let key = self.key.as_deref().ok_or(NotesError::MissingKey)?;
        let body = json!({
            "model": self.model,
            "max_tokens": 16000,
            "system": system,
            "messages": [{ "role": "user", "content": user }],
            "output_config": {
                "effort": "medium",
                "format": { "type": "json_schema", "schema": schema() },
            },
            "fallbacks": "default",
        });
        let res = self
            .http
            .post(ANTHROPIC_URL)
            .header("x-api-key", key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("anthropic-beta", ANTHROPIC_FALLBACK_BETA)
            .json(&body)
            .send()
            .await?;
        let v = checked(res).await?;
        match v.get("stop_reason").and_then(Value::as_str) {
            Some("refusal") => {
                let why = v
                    .pointer("/stop_details/explanation")
                    .and_then(Value::as_str)
                    .unwrap_or("no reason given");
                return Err(NotesError::Provider(format!(
                    "Claude declined this transcript ({why})"
                )));
            }
            Some("max_tokens") => {
                return Err(NotesError::BadOutput(
                    "reply was cut off (max_tokens)".into(),
                ));
            }
            _ => {}
        }
        // Content may include thinking blocks; the JSON is in the text block(s).
        let text: String = v
            .get("content")
            .and_then(Value::as_array)
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect()
            })
            .unwrap_or_default();
        if text.is_empty() {
            return Err(NotesError::BadOutput("empty reply".into()));
        }
        Ok(text)
    }
}

async fn checked(res: reqwest::Response) -> Result<Value, NotesError> {
    let status = res.status();
    let body: Value = res.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(body);
    }
    let msg = body
        .pointer("/error/message")
        .or_else(|| body.get("error"))
        .map(|m| {
            m.as_str()
                .map(String::from)
                .unwrap_or_else(|| m.to_string())
        })
        .unwrap_or_else(|| status.to_string());
    Err(NotesError::Provider(format!("{status}: {msg}")))
}

fn text_at(v: &Value, path: &[&str]) -> Result<String, NotesError> {
    let mut cur = v;
    for p in path {
        cur = match p.parse::<usize>() {
            Ok(i) => cur.get(i),
            Err(_) => cur.get(*p),
        }
        .ok_or_else(|| NotesError::BadOutput(format!("missing field {p}")))?;
    }
    cur.as_str()
        .map(String::from)
        .ok_or_else(|| NotesError::BadOutput("reply is not text".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_ollama_gives_install_hint() {
        // Port 9 (discard) is never an Ollama server.
        let p = Provider::new(
            ProviderKind::Ollama,
            None,
            Some("http://127.0.0.1:9".into()),
            None,
        );
        let err = p.check().await.unwrap_err().to_string();
        assert!(
            err.contains("ollama.com") && err.contains("ollama pull llama3.2"),
            "{err}"
        );
        let err = p.complete("s", "u").await.unwrap_err().to_string();
        assert!(err.contains("Ollama is not running"), "{err}");
        let cloud = Provider::new(ProviderKind::Anthropic, None, None, None);
        assert!(
            cloud
                .check()
                .await
                .unwrap_err()
                .to_string()
                .contains("API key")
        );
    }

    #[test]
    fn defaults_and_paths() {
        let p = Provider::new(ProviderKind::Anthropic, None, None, None);
        assert_eq!(p.label(), ("anthropic", "claude-opus-5-5"));
        let p = Provider::new(
            ProviderKind::Ollama,
            None,
            Some("http://box:11434/".into()),
            None,
        );
        assert_eq!(p.base_url, "http://box:11434");
        assert!(!ProviderKind::Ollama.needs_key());
        assert_eq!(ProviderKind::parse(Some("off")), None);
        let v = json!({"choices": [{"message": {"content": "{}"}}]});
        assert_eq!(
            text_at(&v, &["choices", "0", "message", "content"]).unwrap(),
            "{}"
        );
        assert!(text_at(&v, &["nope"]).is_err());
    }
}
