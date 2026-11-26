//! Import data from TaskMind v1 (Python, Linux): activities, recordings with transcripts, and
//! projects from `projects.yaml`. Read-only on v1 files, idempotent (runs once).
//! v1 stored local times without a timezone; they are converted to UTC here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::db::{Db, NewActivity, RuleRow, SegmentRow};

pub const DONE_SETTING: &str = "import.v1.doneAt";

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub found: bool,
    pub projects: usize,
    pub activities: usize,
    pub meetings: usize,
}

pub fn v1_db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".local/share/taskmind/taskmind.db"))
}

pub fn v1_projects_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".config/taskmind/projects.yaml"))
}

pub fn available() -> bool {
    v1_db_path().is_some_and(|p| p.exists()) || v1_projects_path().is_some_and(|p| p.exists())
}

fn to_utc(local: &str) -> Option<String> {
    let trimmed = local.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(
            dt.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true),
        );
    }
    let naive = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()?;
    let local = Local.from_local_datetime(&naive).earliest()?;
    Some(
        local
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

fn plus_secs(utc: &str, secs: i64) -> String {
    chrono::DateTime::parse_from_rfc3339(utc)
        .map(|d| {
            (d.with_timezone(&Utc) + chrono::Duration::seconds(secs))
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .unwrap_or_else(|_| utc.to_string())
}

#[derive(Deserialize)]
struct V1Projects {
    #[serde(default)]
    projects: Vec<V1Project>,
}

#[derive(Deserialize)]
struct V1Project {
    name: String,
    #[serde(default)]
    matchers: Vec<V1Matcher>,
}

#[derive(Deserialize)]
struct V1Matcher {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    equals: Vec<String>,
    #[serde(default)]
    window_title_contains: Vec<String>,
}

/// v1 matchers become v2 rules. "app equals X AND title contains Y" has no exact v2 form,
/// so it becomes title rules for Y (the more specific half).
fn rules_from(matchers: &[V1Matcher]) -> Vec<RuleRow> {
    let mut rules = Vec::new();
    for m in matchers {
        let rule = |kind: &str, pattern: &String| RuleRow {
            kind: kind.into(),
            pattern: pattern.clone(),
            priority: 0,
        };
        match m.kind.as_str() {
            "window_title" | "git_branch" | "directory" => {
                let kind = m.kind.as_str();
                rules.extend(m.contains.iter().map(|p| rule(kind, p)));
            }
            "app_name" if !m.window_title_contains.is_empty() => {
                rules.extend(
                    m.window_title_contains
                        .iter()
                        .map(|p| rule("window_title", p)),
                );
            }
            "app_name" => rules.extend(m.equals.iter().map(|p| rule("app_name", p))),
            _ => {}
        }
    }
    rules
}

pub fn import(
    db: &Db,
    v1_db: Option<&Path>,
    projects_yaml: Option<&Path>,
) -> Result<ImportSummary, String> {
    let mut summary = ImportSummary::default();
    let now = Utc::now().to_rfc3339();
    let mut project_ids: HashMap<String, i64> = db
        .projects()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| (p.name.to_lowercase(), p.id))
        .collect();

    if let Some(path) = projects_yaml.filter(|p| p.exists()) {
        summary.found = true;
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let parsed: V1Projects =
            serde_yaml_ng::from_str(&text).map_err(|e| format!("projects.yaml: {e}"))?;
        for p in parsed.projects {
            if project_ids.contains_key(&p.name.to_lowercase()) {
                continue;
            }
            let id = db
                .save_project(None, &p.name, None, &rules_from(&p.matchers), &now)
                .map_err(|e| e.to_string())?;
            project_ids.insert(p.name.to_lowercase(), id);
            summary.projects += 1;
        }
    }

    let Some(path) = v1_db.filter(|p| p.exists()) else {
        return Ok(summary);
    };
    summary.found = true;
    let v1 = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    // Activities: merge consecutive identical 10 s samples into one row.
    let mut stmt = v1
        .prepare(
            "SELECT timestamp, COALESCE(window_title,''), COALESCE(app_name,''), COALESCE(project_name,''),
                    COALESCE(duration_seconds,10), COALESCE(is_idle,0), COALESCE(browser_url,'')
             FROM activities ORDER BY timestamp",
        )
        .map_err(|e| e.to_string())?;
    type Row = (String, String, String, String, i64, bool, String);
    let rows = stmt
        .query_map([], |r| -> rusqlite::Result<Row> {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut open: Option<(String, String, Row)> = None; // (start, end, sample)
    let flush = |o: &(String, String, Row), ids: &HashMap<String, i64>| -> Result<(), String> {
        let (start, end, (_, title, app, project, _, idle, url)) = o;
        let project_id = (!project.is_empty() && project != "Unclassified")
            .then(|| ids.get(&project.to_lowercase()).copied())
            .flatten();
        db.insert_activity(&NewActivity {
            started_at: start,
            ended_at: end,
            app_name: app,
            window_title: title,
            url: (!url.is_empty()).then_some(url.as_str()),
            project_id,
            is_idle: *idle,
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
    };
    for row in rows {
        let row = row.map_err(|e| e.to_string())?;
        let Some(start) = to_utc(&row.0) else {
            continue;
        };
        let end = plus_secs(&start, row.4.max(1));
        let same = open.as_ref().is_some_and(|(_, prev_end, s)| {
            s.1 == row.1
                && s.2 == row.2
                && s.3 == row.3
                && s.5 == row.5
                && s.6 == row.6
                && *prev_end >= plus_secs(&start, -30)
        });
        if same {
            if let Some(o) = open.as_mut() {
                o.1 = end;
            }
        } else {
            if let Some(o) = open.take() {
                flush(&o, &project_ids)?;
                summary.activities += 1;
            }
            open = Some((start, end, row));
        }
    }
    if let Some(o) = open.take() {
        flush(&o, &project_ids)?;
        summary.activities += 1;
    }

    // Recordings become meetings. v1 transcripts have no timestamps: one segment each.
    let has_recordings: bool = v1
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'recordings'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n > 0)
        .unwrap_or(false);
    if has_recordings {
        let mut stmt = v1
            .prepare("SELECT started_at, ended_at, duration_seconds, file_path, transcript FROM recordings ORDER BY started_at")
            .map_err(|e| e.to_string())?;
        let recs = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<i64>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for rec in recs {
            let (started, ended, duration, file, transcript) = rec.map_err(|e| e.to_string())?;
            let Some(started) = to_utc(&started) else {
                continue;
            };
            let file = file.unwrap_or_default();
            let id = db
                .create_meeting(&started, &file)
                .map_err(|e| e.to_string())?;
            let ended = ended
                .as_deref()
                .and_then(to_utc)
                .unwrap_or_else(|| plus_secs(&started, duration.unwrap_or(0)));
            let bytes = std::fs::metadata(&file)
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            db.finish_meeting(id, &ended, duration.unwrap_or(0), bytes)
                .map_err(|e| e.to_string())?;
            db.rename_meeting(id, Some("Imported from v1"))
                .map_err(|e| e.to_string())?;
            match transcript.filter(|t| !t.trim().is_empty() && !t.starts_with("[Transcription")) {
                Some(text) => {
                    let seg = SegmentRow {
                        start_ms: 0,
                        end_ms: duration.unwrap_or(0) * 1000,
                        speaker: "unknown".into(),
                        text,
                    };
                    db.save_transcript(id, &[seg], None, "v1")
                        .map_err(|e| e.to_string())?;
                }
                // v1 WAV/OGG files can be transcribed by v2 if they still exist (Opus only).
                None if !file.ends_with(".ogg") || bytes == 0 => {
                    db.set_meeting_status(
                        id,
                        "failed",
                        Some("imported from v1 without a transcript"),
                    )
                    .map_err(|e| e.to_string())?;
                }
                None => {}
            }
            summary.meetings += 1;
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_v1_projects_activities_and_recordings() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = dir.path().join("projects.yaml");
        std::fs::write(
            &yaml,
            "projects:\n  - name: \"Permit\"\n    matchers:\n      - type: window_title\n        contains: [\"permit\"]\n      - type: app_name\n        equals: [\"slack\"]\n",
        )
        .unwrap();
        let v1_path = dir.path().join("v1.db");
        let v1 = Connection::open(&v1_path).unwrap();
        v1.execute_batch(
            "CREATE TABLE activities (id INTEGER PRIMARY KEY, timestamp TEXT, window_title TEXT, app_name TEXT,
               window_class TEXT, project_name TEXT, duration_seconds INTEGER, is_idle INTEGER, browser_url TEXT);
             INSERT INTO activities VALUES (1,'2026-06-11T10:00:00.1','permit.rs','code','', 'Permit',10,0,'');
             INSERT INTO activities VALUES (2,'2026-06-11T10:00:10.1','permit.rs','code','', 'Permit',10,0,'');
             INSERT INTO activities VALUES (3,'2026-06-11T10:00:20.1','inbox','chrome','', 'Unclassified',10,0,'https://mail');
             CREATE TABLE recordings (id INTEGER PRIMARY KEY, started_at TEXT, ended_at TEXT, duration_seconds INTEGER,
               file_path TEXT, transcript TEXT, status TEXT, created_at TEXT);
             INSERT INTO recordings VALUES (1,'2026-06-11T11:00:00','2026-06-11T11:30:00',1800,'/gone.wav','we agreed on friday','done','x');",
        )
        .unwrap();
        drop(v1);

        let db = Db::open_in_memory().unwrap();
        let s = import(&db, Some(&v1_path), Some(&yaml)).unwrap();
        assert!(s.found);
        assert_eq!((s.projects, s.activities, s.meetings), (1, 2, 1));
        let p = db.projects().unwrap();
        assert_eq!(p[0].rules.len(), 2);
        let m = db.list_meetings(10).unwrap();
        assert_eq!(m[0].status, "done");
        assert_eq!(db.search_transcripts("friday", 10).unwrap().len(), 1);
        // Running again does not duplicate projects.
        let s2 = import(&db, None, Some(&yaml)).unwrap();
        assert_eq!(s2.projects, 0);
    }

    #[test]
    fn naive_local_time_converts() {
        assert!(to_utc("2026-06-11T10:00:00.123456").is_some());
        assert!(to_utc("2026-06-11 10:00:00").is_some());
        assert_eq!(
            to_utc("2026-06-11T10:00:00Z").as_deref(),
            Some("2026-06-11T10:00:00Z")
        );
        assert!(to_utc("garbage").is_none());
    }
}
