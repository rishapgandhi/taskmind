//! Day reports, time blocks and timesheets (merge rules ported from v1 `timesheet.py`).
//!
//! - Consecutive non-idle activities with the same project merge into one block if the gap is
//!   at most 2 minutes. Idle ends a block.
//! - Time inside a recorded meeting becomes a meeting block instead of work time, so the
//!   timeline never counts the same minutes twice.
//! - Day boundaries use the user's local timezone (v1 had a UTC/local bug here).

use std::collections::HashMap;

use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::db::{ActivityRow, MeetingRow};

const MERGE_GAP_SECS: i64 = 120;
const MIN_BLOCK_SECS: i64 = 60;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// "work" or "meeting"
    pub kind: &'static str,
    pub project_id: Option<i64>,
    pub meeting_id: Option<i64>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub seconds: i64,
    /// Top window titles / URLs (work) or the meeting title.
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Total {
    pub key: String,
    pub project_id: Option<i64>,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayReport {
    pub date: String,
    pub work_seconds: i64,
    pub meeting_seconds: i64,
    pub idle_seconds: i64,
    pub projects: Vec<Total>,
    pub apps: Vec<Total>,
    pub blocks: Vec<Block>,
}

/// [start, end) of a local calendar day, in UTC.
pub fn day_bounds(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let local = |d: NaiveDate| {
        let naive = d.and_hms_opt(0, 0, 0).unwrap_or_default();
        Local
            .from_local_datetime(&naive)
            .earliest()
            .unwrap_or_else(|| Local.from_utc_datetime(&naive))
            .with_timezone(&Utc)
    };
    let next = date.succ_opt().unwrap_or(date);
    (local(date), local(next))
}

pub fn parse(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn clip(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    let s = start.max(from);
    let e = end.min(to);
    (e > s).then_some((s, e))
}

fn describe(a: &ActivityRow) -> String {
    let label = match &a.url {
        Some(url) if !url.is_empty() => url.clone(),
        _ if !a.window_title.is_empty() => a.window_title.clone(),
        _ => a.app_name.clone(),
    };
    label.chars().take(120).collect()
}

struct Open {
    project_id: Option<i64>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    seconds: i64,
    details: HashMap<String, i64>,
}

impl Open {
    fn close(self) -> Block {
        let mut details: Vec<(String, i64)> = self.details.into_iter().collect();
        details.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        Block {
            kind: "work",
            project_id: self.project_id,
            meeting_id: None,
            start: self.start,
            end: self.end,
            seconds: self.seconds,
            details: details
                .into_iter()
                .take(3)
                .map(|d| d.0)
                .filter(|d| !d.is_empty())
                .collect(),
        }
    }
}

pub fn day_report(
    date: NaiveDate,
    activities: &[ActivityRow],
    meetings: &[MeetingRow],
) -> DayReport {
    let (from, to) = day_bounds(date);

    let meeting_spans: Vec<(i64, String, DateTime<Utc>, DateTime<Utc>)> = meetings
        .iter()
        .filter_map(|m| {
            let s = parse(&m.started_at)?;
            let e = m
                .ended_at
                .as_deref()
                .and_then(parse)
                .unwrap_or_else(Utc::now);
            let (s, e) = clip(s, e, from, to)?;
            Some((
                m.id,
                m.title.clone().unwrap_or_else(|| "Meeting".into()),
                s,
                e,
            ))
        })
        .collect();
    let in_meeting = |t: DateTime<Utc>| meeting_spans.iter().any(|(_, _, s, e)| t >= *s && t < *e);

    let mut blocks = Vec::new();
    let mut open: Option<Open> = None;
    let mut work = 0;
    let mut idle = 0;
    let mut projects: HashMap<Option<i64>, i64> = HashMap::new();
    let mut apps: HashMap<String, i64> = HashMap::new();

    for a in activities {
        let (Some(s), Some(e)) = (parse(&a.started_at), parse(&a.ended_at)) else {
            continue;
        };
        let Some((s, e)) = clip(s, e, from, to) else {
            continue;
        };
        let secs = (e - s).num_seconds();
        if a.is_idle {
            idle += secs;
            if let Some(o) = open.take() {
                blocks.push(o.close());
            }
            continue;
        }
        if in_meeting(s + (e - s) / 2) {
            continue; // counted as meeting time
        }
        work += secs;
        *projects.entry(a.project_id).or_default() += secs;
        if !a.app_name.is_empty() {
            *apps.entry(a.app_name.clone()).or_default() += secs;
        }
        let extend = open.as_ref().is_some_and(|o| {
            o.project_id == a.project_id && (s - o.end).num_seconds() <= MERGE_GAP_SECS
        });
        if !extend && let Some(o) = open.take() {
            blocks.push(o.close());
        }
        let o = open.get_or_insert_with(|| Open {
            project_id: a.project_id,
            start: s,
            end: e,
            seconds: 0,
            details: HashMap::new(),
        });
        o.end = o.end.max(e);
        o.seconds += secs;
        *o.details.entry(describe(a)).or_default() += secs;
    }
    if let Some(o) = open.take() {
        blocks.push(o.close());
    }
    blocks.retain(|b| b.seconds >= MIN_BLOCK_SECS);

    let mut meeting_seconds = 0;
    for (id, title, s, e) in &meeting_spans {
        let secs = (*e - *s).num_seconds();
        meeting_seconds += secs;
        blocks.push(Block {
            kind: "meeting",
            project_id: None,
            meeting_id: Some(*id),
            start: *s,
            end: *e,
            seconds: secs,
            details: vec![title.clone()],
        });
    }
    blocks.sort_by_key(|b| b.start);

    let totals = |map: HashMap<String, i64>, ids: HashMap<String, Option<i64>>| {
        let mut v: Vec<Total> = map
            .into_iter()
            .map(|(key, seconds)| Total {
                project_id: ids.get(&key).copied().flatten(),
                key,
                seconds,
            })
            .collect();
        v.sort_by(|a, b| b.seconds.cmp(&a.seconds).then(a.key.cmp(&b.key)));
        v
    };
    let project_keys: HashMap<String, Option<i64>> = projects
        .keys()
        .map(|id| (id.map_or("none".into(), |i| i.to_string()), *id))
        .collect();
    let project_totals: HashMap<String, i64> = projects
        .into_iter()
        .map(|(id, s)| (id.map_or("none".into(), |i| i.to_string()), s))
        .collect();
    let mut app_totals = totals(apps, HashMap::new());
    app_totals.truncate(10);

    DayReport {
        date: date.to_string(),
        work_seconds: work,
        meeting_seconds,
        idle_seconds: idle,
        projects: totals(project_totals, project_keys),
        apps: app_totals,
        blocks,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub project_id: Option<i64>,
    pub project_name: String,
    /// Local "HH:MM"
    pub start: String,
    pub end: String,
    pub minutes: i64,
    pub description: String,
}

/// Draft timesheet entries from a day report (work blocks and meetings).
pub fn draft_entries(
    report: &DayReport,
    project_name: impl Fn(Option<i64>) -> String,
) -> Vec<Entry> {
    report
        .blocks
        .iter()
        .map(|b| Entry {
            project_id: b.project_id,
            project_name: if b.kind == "meeting" {
                "Meetings".into()
            } else {
                project_name(b.project_id)
            },
            start: b.start.with_timezone(&Local).format("%H:%M").to_string(),
            end: b.end.with_timezone(&Local).format("%H:%M").to_string(),
            minutes: (b.seconds + 30) / 60,
            description: b.details.join("; "),
        })
        .collect()
}

/// CSV in the v1 column layout: date, project_name, start_time, end_time, duration_minutes, description.
pub fn to_csv(rows: &[(String, Entry)]) -> String {
    fn field(s: &str) -> String {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
    let mut out =
        String::from("date,project_name,start_time,end_time,duration_minutes,description\n");
    for (date, e) in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            date,
            field(&e.project_name),
            e.start,
            e.end,
            e.minutes,
            field(&e.description)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn act(start: &str, end: &str, project: Option<i64>, idle: bool, title: &str) -> ActivityRow {
        ActivityRow {
            id: 0,
            started_at: start.into(),
            ended_at: end.into(),
            app_name: "Code".into(),
            window_title: title.into(),
            url: None,
            project_id: project,
            is_idle: idle,
        }
    }

    fn today_at(h: u32, m: u32) -> String {
        let date = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        Local
            .from_local_datetime(&date.and_hms_opt(h, m, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc)
            .to_rfc3339()
    }

    #[test]
    fn merges_splits_and_counts() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        let acts = vec![
            act(&today_at(9, 0), &today_at(9, 30), Some(1), false, "a.rs"),
            act(&today_at(9, 31), &today_at(10, 0), Some(1), false, "b.rs"), // 1 min gap: merged
            act(&today_at(10, 0), &today_at(10, 10), None, true, ""),        // idle ends block
            act(&today_at(10, 10), &today_at(10, 40), Some(1), false, "a.rs"),
            act(&today_at(10, 40), &today_at(11, 0), Some(2), false, "c.rs"),
            act(&today_at(11, 0), &today_at(11, 0), Some(2), false, "tiny"),
        ];
        let meetings = vec![MeetingRow {
            id: 7,
            title: Some("Standup".into()),
            started_at: today_at(10, 30),
            ended_at: Some(today_at(10, 45)),
            duration_s: Some(900),
            audio_path: None,
            audio_bytes: None,
            status: "done".into(),
            language: None,
            error: None,
            has_notes: false,
            notes_error: None,
        }];
        let r = day_report(date, &acts, &meetings);
        assert_eq!(r.idle_seconds, 600);
        assert_eq!(r.meeting_seconds, 900);
        let kinds: Vec<(&str, Option<i64>)> =
            r.blocks.iter().map(|b| (b.kind, b.project_id)).collect();
        assert_eq!(
            kinds,
            vec![
                ("work", Some(1)),
                ("work", Some(1)),
                ("meeting", None),
                ("work", Some(2))
            ]
        );
        assert_eq!(r.blocks[0].seconds, 59 * 60);
        assert_eq!(r.blocks[0].details[0], "a.rs");
        // 10:10-10:40 work overlaps the 10:30 meeting only partly; its midpoint (10:25) is outside.
        assert_eq!(r.blocks[1].seconds, 30 * 60);

        let entries = draft_entries(&r, |id| format!("P{}", id.unwrap_or(0)));
        assert_eq!(entries[0].start, "09:00");
        assert_eq!(entries[0].minutes, 59);
        assert_eq!(entries[2].project_name, "Meetings");
    }

    #[test]
    fn clips_to_local_day() {
        let date = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
        let (from, to) = day_bounds(date);
        assert_eq!((to - from).num_hours(), 24);
        let acts = vec![act(
            &(from - chrono::Duration::minutes(30)).to_rfc3339(),
            &(from + chrono::Duration::minutes(30)).to_rfc3339(),
            None,
            false,
            "x",
        )];
        let r = day_report(date, &acts, &[]);
        assert_eq!(r.work_seconds, 30 * 60);
    }

    #[test]
    fn csv_escaping() {
        let e = Entry {
            project_id: None,
            project_name: "A, B".into(),
            start: "09:00".into(),
            end: "10:00".into(),
            minutes: 60,
            description: "say \"hi\"".into(),
        };
        let csv = to_csv(&[("2026-11-27".into(), e)]);
        assert!(csv.ends_with("2026-11-27,\"A, B\",09:00,10:00,60,\"say \"\"hi\"\"\"\n"));
    }
}
