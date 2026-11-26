//! Whisper model catalog, download (with SHA-256 check) and removal.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use super::TranscribeError;

pub const PROGRESS_EVENT: &str = "models://progress";
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

pub struct ModelInfo {
    pub id: &'static str,
    pub file: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub size_bytes: u64,
    pub sha256: &'static str,
}

/// Multilingual models only, so Hindi, English and mixed meetings all work.
/// Sizes and hashes from the Hugging Face API (ggerganov/whisper.cpp, 2026-11-27).
pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "tiny",
        file: "ggml-tiny.bin",
        label: "Tiny",
        description: "Fastest, lowest accuracy. For machines with 4 GB RAM.",
        size_bytes: 77_691_713,
        sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
    },
    ModelInfo {
        id: "base",
        file: "ggml-base.bin",
        label: "Base",
        description: "Good balance for most laptops. Recommended default.",
        size_bytes: 147_951_465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
    },
    ModelInfo {
        id: "small",
        file: "ggml-small-q5_1.bin",
        label: "Small",
        description: "More accurate, about 2x slower than Base.",
        size_bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
    },
    ModelInfo {
        id: "turbo",
        file: "ggml-large-v3-turbo-q5_0.bin",
        label: "Large v3 Turbo",
        description: "Best accuracy. Needs 8 GB+ RAM and a fast CPU.",
        size_bytes: 574_041_195,
        sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
    },
];

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn path(dir: &Path, model: &ModelInfo) -> PathBuf {
    dir.join(model.file)
}

/// `tiny` below 8 GB of RAM, otherwise `base`.
pub fn recommended() -> &'static str {
    let total = sysinfo::System::new()
        .map(|mut sys| {
            sys.refresh_memory();
            sys.total_memory()
        })
        .unwrap_or(u64::MAX);
    if total < 7 * 1024 * 1024 * 1024 {
        "tiny"
    } else {
        "base"
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    size_bytes: u64,
    downloaded: bool,
    recommended: bool,
}

pub fn list(dir: &Path) -> Vec<ModelStatus> {
    let rec = recommended();
    MODELS
        .iter()
        .map(|m| ModelStatus {
            id: m.id,
            label: m.label,
            description: m.description,
            size_bytes: m.size_bytes,
            downloaded: is_downloaded(dir, m),
            recommended: m.id == rec,
        })
        .collect()
}

pub fn is_downloaded(dir: &Path, model: &ModelInfo) -> bool {
    std::fs::metadata(path(dir, model)).is_ok_and(|m| m.len() == model.size_bytes)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    id: &'static str,
    downloaded: u64,
    total: u64,
    done: bool,
    error: Option<String>,
}

/// Download to `<file>.part`, verify size and SHA-256, then rename. Emits progress events.
pub async fn download(
    app: &AppHandle,
    dir: &Path,
    model: &'static ModelInfo,
) -> Result<(), TranscribeError> {
    let result = download_inner(app, dir, model).await;
    let _ = app.emit(
        PROGRESS_EVENT,
        Progress {
            id: model.id,
            downloaded: if result.is_ok() { model.size_bytes } else { 0 },
            total: model.size_bytes,
            done: true,
            error: result.as_ref().err().map(ToString::to_string),
        },
    );
    result
}

async fn download_inner(
    app: &AppHandle,
    dir: &Path,
    model: &'static ModelInfo,
) -> Result<(), TranscribeError> {
    tokio::fs::create_dir_all(dir).await?;
    let final_path = path(dir, model);
    let part = final_path.with_extension("bin.part");

    let response = reqwest::Client::new()
        .get(format!("{BASE_URL}{}", model.file))
        .send()
        .await?
        .error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&part).await?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 2 * 1024 * 1024 {
            last_emit = downloaded;
            let _ = app.emit(
                PROGRESS_EVENT,
                Progress {
                    id: model.id,
                    downloaded,
                    total: model.size_bytes,
                    done: false,
                    error: None,
                },
            );
        }
    }
    file.flush().await?;
    drop(file);

    let digest = hex::encode(hasher.finalize());
    if downloaded != model.size_bytes || digest != model.sha256 {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(TranscribeError::BadDownload(format!(
            "{} failed verification (got {downloaded} bytes, sha256 {digest})",
            model.file
        )));
    }
    tokio::fs::rename(&part, &final_path).await?;
    tracing::info!(model = model.id, "model downloaded and verified");
    Ok(())
}

pub fn delete(dir: &Path, model: &ModelInfo) -> Result<(), TranscribeError> {
    match std::fs::remove_file(path(dir, model)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        for m in MODELS {
            assert_eq!(m.sha256.len(), 64, "{}", m.id);
            assert!(m.file.ends_with(".bin"));
            assert!(find(m.id).is_some());
        }
        assert!(find(recommended()).is_some());
    }

    #[test]
    fn partial_file_is_not_downloaded() {
        let dir = tempfile::tempdir().unwrap();
        let m = find("tiny").unwrap();
        assert!(!is_downloaded(dir.path(), m));
        std::fs::write(path(dir.path(), m), b"short").unwrap();
        assert!(!is_downloaded(dir.path(), m));
    }
}
