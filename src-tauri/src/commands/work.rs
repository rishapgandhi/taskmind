//! Work tracking, projects, reports, timesheets and export.

use std::collections::HashMap;

use chrono::{Local, NaiveDate, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::classify::Classifier;
use crate::db::{Db, ProjectRow, RULE_KINDS, RuleRow};
use crate::error::{CommandError, CommandResult};
use crate::timesheet::{self, DayReport, Entry};
use crate::tracker::{Tracker, TrackerStatus};

const RECLASSIFY_DAYS: i64 = 30;
const MAX_RANGE_DAYS: i64 = 62;

fn date(s: &str) -> CommandResult<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| CommandError::new("invalid", "date must be YYYY-MM-DD"))
}

fn iso(d: chrono::DateTime<Utc>) -> String {
    d.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn report_for(db: &Db, day: NaiveDate) -> CommandResult<DayReport> {
    let (from, to) = timesheet::day_bounds(day);
    let acts = db.activities_between(&iso(from), &iso(to))?;
    let meetings = db.meetings_between(&iso(from), &iso(to))?;
    Ok(timesheet::day_report(day, &acts, &meetings))
}

#[tauri::command]
pub fn tracking_status(t: State<'_, Tracker>) -> TrackerStatus {
    t.status()
}

#[tauri::command]
pub fn request_tracking_permission() {
    crate::activity::request_permission();
}

#[tauri::command]
pub fn day_report(db: State<'_, Db>, date_str: String) -> CommandResult<DayReport> {
    report_for(&db, date(&date_str)?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaySummary {
    date: String,
    work_seconds: i64,
    meeting_seconds: i64,
    projects: Vec<timesheet::Total>,
}

#[tauri::command]
pub fn range_report(db: State<'_, Db>, from: String, to: String) -> CommandResult<Vec<DaySummary>> {
    let (from, to) = (date(&from)?, date(&to)?);
    if to < from || (to - from).num_days() > MAX_RANGE_DAYS {
        return Err(CommandError::new("invalid", "range must be 1 to 62 days"));
    }
    let mut out = Vec::new();
    let mut day = from;
    while day <= to {
        let r = report_for(&db, day)?;
        out.push(DaySummary {
            date: r.date,
            work_seconds: r.work_seconds,
            meeting_seconds: r.meeting_seconds,
            projects: r.projects,
        });
        day = day.succ_opt().unwrap_or(to + chrono::Duration::days(1));
    }
    Ok(out)
}

/// Delete all tracked activity for a day (privacy: everything is deletable from the UI).
#[tauri::command]
pub fn delete_day_activity(
    db: State<'_, Db>,
    t: State<'_, Tracker>,
    date_str: String,
) -> CommandResult<usize> {
    let (from, to) = timesheet::day_bounds(date(&date_str)?);
    let n = db.delete_activities_between(&iso(from), &iso(to))?;
    t.wake();
    Ok(n)
}

#[tauri::command]
pub fn list_projects(db: State<'_, Db>) -> CommandResult<Vec<ProjectRow>> {
    Ok(db.projects()?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInput {
    id: Option<i64>,
    name: String,
    color: Option<String>,
    rules: Vec<RuleRow>,
}

fn reclassify(db: &Db) -> CommandResult<()> {
    let classifier = Classifier::new(&db.projects()?);
    let since = iso(Utc::now() - chrono::Duration::days(RECLASSIFY_DAYS));
    db.reclassify_since(&since, |app, title, url| {
        classifier.classify(app, title, url)
    })?;
    Ok(())
}

#[tauri::command]
pub fn save_project(
    db: State<'_, Db>,
    t: State<'_, Tracker>,
    project: ProjectInput,
) -> CommandResult<i64> {
    let name = project.name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(CommandError::new(
            "invalid",
            "project name must be 1 to 80 characters",
        ));
    }
    if let Some(other) = db.project_by_name(name)?
        && Some(other) != project.id
    {
        return Err(CommandError::new(
            "invalid",
            format!("a project named '{name}' already exists"),
        ));
    }
    let rules: Vec<RuleRow> = project
        .rules
        .into_iter()
        .filter(|r| !r.pattern.trim().is_empty())
        .map(|r| RuleRow {
            pattern: r.pattern.trim().chars().take(200).collect(),
            ..r
        })
        .collect();
    if let Some(bad) = rules
        .iter()
        .find(|r| !RULE_KINDS.contains(&r.kind.as_str()))
    {
        return Err(CommandError::new(
            "invalid",
            format!("unknown rule type '{}'", bad.kind),
        ));
    }
    let id = db.save_project(
        project.id,
        name,
        project.color.as_deref(),
        &rules,
        &iso(Utc::now()),
    )?;
    reclassify(&db)?;
    t.wake();
    Ok(id)
}

#[tauri::command]
pub fn delete_project(db: State<'_, Db>, t: State<'_, Tracker>, id: i64) -> CommandResult<()> {
    db.delete_project(id)?;
    reclassify(&db)?;
    t.wake();
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timesheet {
    date: String,
    /// "generated" (not saved yet), "draft" or "approved"
    status: String,
    entries: Vec<Entry>,
}

fn project_names(db: &Db) -> CommandResult<HashMap<i64, String>> {
    Ok(db.projects()?.into_iter().map(|p| (p.id, p.name)).collect())
}

fn timesheet_for(db: &Db, day: NaiveDate) -> CommandResult<Timesheet> {
    let key = day.to_string();
    if let Some((status, entries)) = db.saved_timesheet(&key)? {
        return Ok(Timesheet {
            date: key,
            status,
            entries,
        });
    }
    let names = project_names(db)?;
    let report = report_for(db, day)?;
    let entries = timesheet::draft_entries(&report, |id| {
        id.and_then(|i| names.get(&i).cloned())
            .unwrap_or_else(|| "Unassigned".into())
    });
    Ok(Timesheet {
        date: key,
        status: "generated".into(),
        entries,
    })
}

#[tauri::command]
pub fn get_timesheet(db: State<'_, Db>, date_str: String) -> CommandResult<Timesheet> {
    timesheet_for(&db, date(&date_str)?)
}

#[tauri::command]
pub fn save_timesheet(
    db: State<'_, Db>,
    date_str: String,
    entries: Vec<Entry>,
    approved: bool,
) -> CommandResult<()> {
    let day = date(&date_str)?;
    for e in &entries {
        let valid_time = |t: &str| chrono::NaiveTime::parse_from_str(t, "%H:%M").is_ok();
        if !valid_time(&e.start) || !valid_time(&e.end) || e.minutes < 0 || e.minutes > 24 * 60 {
            return Err(CommandError::new(
                "invalid",
                "times must be HH:MM and minutes 0 to 1440",
            ));
        }
    }
    Ok(db.save_timesheet(&day.to_string(), &entries, approved)?)
}

#[tauri::command]
pub fn reset_timesheet(db: State<'_, Db>, date_str: String) -> CommandResult<()> {
    Ok(db.clear_timesheet(&date(&date_str)?.to_string())?)
}

/// Write the timesheet for a date range to the Downloads folder and show it.
#[tauri::command]
pub fn export_timesheet(
    app: AppHandle,
    db: State<'_, Db>,
    from: String,
    to: String,
    format: String,
) -> CommandResult<String> {
    let (from, to) = (date(&from)?, date(&to)?);
    if to < from || (to - from).num_days() > 366 {
        return Err(CommandError::new("invalid", "range must be 1 to 366 days"));
    }
    let mut rows: Vec<(String, Entry)> = Vec::new();
    let mut day = from;
    while day <= to {
        for e in timesheet_for(&db, day)?.entries {
            rows.push((day.to_string(), e));
        }
        day = day.succ_opt().unwrap_or(to + chrono::Duration::days(1));
    }
    let (ext, body) = match format.as_str() {
        "csv" => ("csv", timesheet::to_csv(&rows)),
        "json" => {
            let items: Vec<serde_json::Value> = rows
                .iter()
                .map(|(d, e)| {
                    serde_json::json!({
                        "date": d, "project_name": e.project_name, "start_time": e.start,
                        "end_time": e.end, "duration_minutes": e.minutes, "description": e.description,
                    })
                })
                .collect();
            (
                "json",
                serde_json::to_string_pretty(&items).unwrap_or_default(),
            )
        }
        _ => return Err(CommandError::new("invalid", "format must be csv or json")),
    };
    let dir = dirs::download_dir()
        .or_else(|| app.path().app_data_dir().ok())
        .ok_or_else(|| CommandError::new("io", "no folder to save to"))?;
    let stamp = Local::now().format("%H%M%S");
    let path = dir.join(format!("taskmind-timesheet-{from}-to-{to}-{stamp}.{ext}"));
    std::fs::write(&path, body).map_err(|e| CommandError::new("io", e.to_string()))?;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.display().to_string())
}
