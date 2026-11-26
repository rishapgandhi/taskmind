//! Where recordings live, audio retention, and backup / restore.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{Local, Utc};
use tauri::{AppHandle, Manager};

use crate::db::Db;
use crate::settings;

pub const DB_FILE: &str = "taskmind.db";
const RESTORE_FILE: &str = "taskmind.db.restore";

/// Default folder: `<app data>/recordings`. Users can pick any folder, for example one synced
/// by the Google Drive or OneDrive desktop app, so recordings reach the cloud with no extra login.
pub struct RecordingPaths {
    pub default_root: PathBuf,
}

pub fn recordings_root(app: &AppHandle) -> PathBuf {
    let custom = app
        .try_state::<Db>()
        .and_then(|db| settings::get_string(&db, settings::STORAGE_RECORDINGS_DIR))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    custom.unwrap_or_else(|| app.state::<RecordingPaths>().default_root.clone())
}

/// Let the webview play files from the chosen folder (the static scope covers only the default).
pub fn allow_playback(app: &AppHandle) {
    let root = recordings_root(app);
    if let Err(err) = app.asset_protocol_scope().allow_directory(&root, true) {
        tracing::warn!(%err, path = %root.display(), "could not allow playback folder");
    }
}

/// Delete audio older than the configured number of days. Transcripts and notes are kept.
pub fn apply_retention(db: &Db) {
    let days = settings::get_u64(db, settings::STORAGE_DELETE_AUDIO_DAYS, 0);
    if days == 0 {
        return;
    }
    let before =
        (Utc::now() - chrono::Duration::days(i64::try_from(days).unwrap_or(0))).to_rfc3339();
    match db.meetings_with_audio_before(&before) {
        Ok(rows) => {
            for (id, path) in rows {
                let p = Path::new(&path);
                let (mic, sys) = crate::audio::track_paths(p);
                for f in [p.to_path_buf(), mic, sys] {
                    let _ = std::fs::remove_file(f);
                }
                if let Err(err) = db.clear_audio(id) {
                    tracing::warn!(%err, id, "could not mark audio as deleted");
                } else {
                    tracing::info!(id, "deleted old meeting audio (retention)");
                }
            }
        }
        Err(err) => tracing::warn!(%err, "retention query failed"),
    }
}

/// Zip with a consistent copy of the database (settings, projects, activity, transcripts, notes).
/// Audio is not included: it is large and already lives in the recordings folder.
pub fn backup(db: &Db, out_dir: &Path) -> Result<PathBuf, String> {
    let tmp = std::env::temp_dir().join(format!("taskmind-backup-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&tmp);
    db.vacuum_into(&tmp).map_err(|e| e.to_string())?;
    let name = format!(
        "taskmind-backup-{}.zip",
        Local::now().format("%Y-%m-%d-%H%M")
    );
    let path = out_dir.join(name);
    let result = (|| -> Result<(), String> {
        let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("manifest.json", opts)
            .map_err(|e| e.to_string())?;
        let manifest = serde_json::json!({
            "app": "TaskMind",
            "version": env!("CARGO_PKG_VERSION"),
            "createdAt": Utc::now().to_rfc3339(),
        });
        zip.write_all(manifest.to_string().as_bytes())
            .map_err(|e| e.to_string())?;
        zip.start_file(DB_FILE, opts).map_err(|e| e.to_string())?;
        let mut src = std::fs::File::open(&tmp).map_err(|e| e.to_string())?;
        std::io::copy(&mut src, &mut zip).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&tmp);
    result.map(|()| path)
}

/// Check a backup and stage it; it replaces the database on the next start.
pub fn stage_restore(zip_path: &Path, data_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|_| "not a TaskMind backup (zip expected)".to_string())?;
    let mut entry = archive
        .by_name(DB_FILE)
        .map_err(|_| "not a TaskMind backup (no database inside)".to_string())?;
    let staged = data_dir.join(RESTORE_FILE);
    let mut out = std::fs::File::create(&staged).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    out.write_all(&buf).map_err(|e| e.to_string())?;
    drop(out);
    // Make sure it opens and migrates before replacing anything.
    if let Err(err) = Db::open(&staged) {
        let _ = std::fs::remove_file(&staged);
        return Err(format!("backup database is not usable: {err}"));
    }
    Ok(())
}

/// Called at startup before the database opens.
pub fn apply_staged_restore(data_dir: &Path) {
    let staged = data_dir.join(RESTORE_FILE);
    if !staged.exists() {
        return;
    }
    let current = data_dir.join(DB_FILE);
    let keep = data_dir.join(format!(
        "taskmind.db.before-restore-{}",
        Local::now().format("%Y%m%d%H%M%S")
    ));
    for suffix in ["", "-wal", "-shm"] {
        let from = PathBuf::from(format!("{}{suffix}", current.display()));
        if from.exists() {
            let to = PathBuf::from(format!("{}{suffix}", keep.display()));
            let _ = std::fs::rename(from, to);
        }
    }
    match std::fs::rename(&staged, &current) {
        Ok(()) => tracing::info!(previous = %keep.display(), "restored database from backup"),
        Err(err) => tracing::error!(%err, "could not apply restore"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_then_restore_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join(DB_FILE)).unwrap();
        db.set_setting("x", &serde_json::json!(42)).unwrap();
        let zip = backup(&db, dir.path()).unwrap();
        db.set_setting("x", &serde_json::json!(1)).unwrap();
        drop(db);

        let restore_dir = dir.path().join("r");
        std::fs::create_dir_all(&restore_dir).unwrap();
        std::fs::copy(dir.path().join(DB_FILE), restore_dir.join(DB_FILE)).unwrap();
        stage_restore(&zip, &restore_dir).unwrap();
        apply_staged_restore(&restore_dir);
        let db = Db::open(&restore_dir.join(DB_FILE)).unwrap();
        assert_eq!(db.get_setting("x").unwrap(), Some(serde_json::json!(42)));

        let bad = dir.path().join("bad.zip");
        std::fs::write(&bad, b"nope").unwrap();
        assert!(stage_restore(&bad, &restore_dir).is_err());
    }
}
