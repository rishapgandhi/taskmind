//! Work tracking loop: samples the active window every 10 s, applies the blocklist before
//! anything is stored (principle 3), classifies into projects, merges unchanged samples into
//! one row, and runs meeting detection.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::activity::{self, ActivitySource, BrowserUrls};
use crate::classify::Classifier;
use crate::db::{Db, NewActivity};
use crate::detect::{self, Detector, Event, Mode};
use crate::recorder::{self, Action, Status};
use crate::{settings, windows};

pub const INTERVAL: Duration = Duration::from_secs(10);
pub const STATUS_EVENT: &str = "tracking://status";
pub const DETECTED_EVENT: &str = "meeting://detected";

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerStatus {
    pub enabled: bool,
    pub idle: bool,
    pub problem: Option<String>,
    pub app_name: Option<String>,
    pub window_title: Option<String>,
    pub project_id: Option<i64>,
}

enum Msg {
    Wake,
    Shutdown,
}

pub struct Tracker {
    tx: Sender<Msg>,
    status: Arc<Mutex<TrackerStatus>>,
}

impl Tracker {
    pub fn start(app: AppHandle, temp_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(TrackerStatus::default()));
        let shared = status.clone();
        let spawned = std::thread::Builder::new()
            .name("taskmind-tracker".into())
            .spawn(move || run(app, rx, shared, temp_dir));
        if let Err(err) = spawned {
            tracing::error!(%err, "could not start tracker thread");
        }
        Self { tx, status }
    }

    /// Re-read settings and rules now.
    pub fn wake(&self) {
        let _ = self.tx.send(Msg::Wake);
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(Msg::Shutdown);
    }

    pub fn status(&self) -> TrackerStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

/// The row being extended while the sample stays the same.
struct Current {
    id: i64,
    key: (String, String, Option<String>, bool, Option<i64>),
}

fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn iso_ago(seconds: u64) -> String {
    (Utc::now() - chrono::Duration::seconds(i64::try_from(seconds).unwrap_or(0)))
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn blocked(blocklist: &[String], app: &str, title: &str, url: Option<&str>) -> bool {
    let (app, title, url) = (
        app.to_lowercase(),
        title.to_lowercase(),
        url.unwrap_or_default().to_lowercase(),
    );
    blocklist.iter().any(|b| {
        let b = b.trim().to_lowercase();
        !b.is_empty() && (app.contains(&b) || title.contains(&b) || url.contains(&b))
    })
}

fn run(app: AppHandle, rx: Receiver<Msg>, status: Arc<Mutex<TrackerStatus>>, temp_dir: PathBuf) {
    let mut source: Box<dyn ActivitySource> = activity::source();
    let mut urls: Option<BrowserUrls> = None;
    let mut current: Option<Current> = None;
    let mut detector = Detector::default();
    let mut auto_started = false;
    let mut ticks: u64 = 0;
    let mut problem: Option<String> = None;

    loop {
        match rx.recv_timeout(INTERVAL) {
            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(Msg::Wake) => current = None, // settings or rules changed: start a fresh row
            Err(RecvTimeoutError::Timeout) => {}
        }
        ticks += 1;
        let Some(db) = app.try_state::<Db>() else {
            continue;
        };

        // Check for permission/extension problems on start and every 5 minutes.
        if ticks == 1 || ticks.is_multiple_of(30) {
            problem = source.problem();
        }
        // Audio retention: shortly after start, then hourly.
        if ticks == 3 || ticks.is_multiple_of(360) {
            crate::storage::apply_retention(&db);
        }

        let enabled = settings::get_bool(&db, settings::TRACKING_ENABLED, true);
        if enabled {
            track_once(
                &app,
                &db,
                source.as_mut(),
                &mut urls,
                &temp_dir,
                &mut current,
                &status,
                problem.clone(),
            );
        } else {
            current = None;
            publish(
                &app,
                &status,
                TrackerStatus {
                    enabled: false,
                    problem: problem.clone(),
                    ..Default::default()
                },
            );
        }

        detect_meetings(&app, &db, source.as_mut(), &mut detector, &mut auto_started);
    }
}

#[allow(clippy::too_many_arguments)]
fn track_once(
    app: &AppHandle,
    db: &Db,
    source: &mut dyn ActivitySource,
    urls: &mut Option<BrowserUrls>,
    temp_dir: &std::path::Path,
    current: &mut Option<Current>,
    status: &Arc<Mutex<TrackerStatus>>,
    problem: Option<String>,
) {
    let idle_limit = settings::get_u64(db, settings::TRACKING_IDLE_MINUTES, 5).clamp(1, 120) * 60;
    let idle_secs = source.idle_seconds().unwrap_or(0);
    let idle = idle_secs >= idle_limit;
    let blocklist = settings::get_string_list(db, settings::TRACKING_BLOCKLIST);

    let (app_name, title, url) = if idle {
        (String::new(), String::new(), None)
    } else {
        let info = source.active().unwrap_or_default();
        let url = if settings::get_bool(db, settings::TRACKING_BROWSER_URLS, false) {
            urls.get_or_insert_with(|| BrowserUrls::new(temp_dir))
                .lookup(&info.app_name, &info.window_title)
        } else {
            *urls = None;
            None
        };
        if blocked(
            &blocklist,
            &info.app_name,
            &info.window_title,
            url.as_deref(),
        ) {
            ("Private".to_string(), String::new(), None)
        } else {
            (info.app_name, info.window_title, url)
        }
    };

    let project_id = if idle || app_name.is_empty() {
        None
    } else {
        match db.projects() {
            Ok(p) => Classifier::new(&p).classify(&app_name, &title, url.as_deref()),
            Err(err) => {
                tracing::warn!(%err, "could not load projects");
                None
            }
        }
    };

    let key = (
        app_name.clone(),
        title.clone(),
        url.clone(),
        idle,
        project_id,
    );
    let now = now_iso();
    let end = (Utc::now() + chrono::Duration::from_std(INTERVAL).unwrap_or_default())
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let same = current.as_ref().is_some_and(|c| c.key == key);
    let result = if same {
        let id = current.as_ref().map(|c| c.id).unwrap_or_default();
        db.extend_activity(id, &end).map(|()| id)
    } else {
        if idle && let Some(c) = current.as_ref() {
            // The last `idle_secs` were already idle: end the previous row when input stopped.
            let _ = db.extend_activity(c.id, &iso_ago(idle_secs));
        }
        let start = if idle { iso_ago(idle_secs) } else { now };
        db.insert_activity(&NewActivity {
            started_at: &start,
            ended_at: &end,
            app_name: &app_name,
            window_title: &title,
            url: url.as_deref(),
            project_id,
            is_idle: idle,
        })
    };
    match result {
        Ok(id) => *current = Some(Current { id, key }),
        Err(err) => {
            tracing::warn!(%err, "could not store activity");
            *current = None;
        }
    }

    publish(
        app,
        status,
        TrackerStatus {
            enabled: true,
            idle,
            problem,
            app_name: (!idle).then_some(app_name),
            window_title: (!idle).then_some(title),
            project_id,
        },
    );
}

fn publish(app: &AppHandle, status: &Arc<Mutex<TrackerStatus>>, next: TrackerStatus) {
    if let Ok(mut s) = status.lock() {
        *s = next.clone();
    }
    let _ = app.emit(STATUS_EVENT, next);
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Detected {
    title: String,
}

fn detect_meetings(
    app: &AppHandle,
    db: &Db,
    source: &mut dyn ActivitySource,
    detector: &mut Detector,
    auto_started: &mut bool,
) {
    let mode = Mode::parse(settings::get_string(db, settings::MEETING_DETECTION).as_deref());
    if mode == Mode::Off {
        return;
    }
    let keywords = {
        let list = settings::get_string_list(db, settings::MEETING_KEYWORDS);
        if list.is_empty() {
            detect::DEFAULT_KEYWORDS
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            list
        }
    };
    let windows = source.meeting_candidates();
    let matched = detect::find_match(windows.iter().map(|w| w.window_title.as_str()), &keywords);
    let recording = app
        .try_state::<recorder::RecorderHandle>()
        .and_then(|h| h.snapshot().ok())
        .is_some_and(|s| s.status != Status::Idle);

    match detector.update(matched) {
        Some(Event::Detected { title }) if !recording => {
            tracing::info!(%title, ?mode, "meeting detected");
            if mode == Mode::Auto {
                match recorder::dispatch(app, Action::Start) {
                    Ok(snapshot) => {
                        *auto_started = true;
                        if let Some(id) = snapshot.meeting_id {
                            let _ = db.rename_meeting(id, Some(&title));
                        }
                    }
                    Err(err) => tracing::warn!(%err, "auto-record failed"),
                }
            } else {
                windows::show_widget(app);
                let _ = app.emit(DETECTED_EVENT, Detected { title });
            }
        }
        Some(Event::Ended) => {
            if *auto_started && recording {
                tracing::info!("meeting window closed, stopping auto-started recording");
                if let Err(err) = recorder::dispatch(app, Action::Stop) {
                    tracing::warn!(%err, "auto-stop failed");
                }
            }
            *auto_started = false;
        }
        _ => {}
    }
    if !recording && *auto_started {
        // The user stopped it by hand.
        *auto_started = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocklist_matches_app_title_or_url() {
        let list = vec!["1password".to_string(), "bank".to_string(), " ".to_string()];
        assert!(blocked(&list, "1Password 8", "Vault", None));
        assert!(blocked(&list, "Chrome", "My Bank - login", None));
        assert!(blocked(
            &list,
            "Chrome",
            "Home",
            Some("https://bank.example")
        ));
        assert!(!blocked(&list, "Code", "main.rs", None));
        assert!(!blocked(&[], "Code", "main.rs", None));
    }
}
