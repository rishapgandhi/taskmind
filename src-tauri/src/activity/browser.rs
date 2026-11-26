//! Browser URL for the active tab, found by matching the window title against the browser's
//! own history database (ported from v1). Opt-in (`tracking.browserUrls`), read-only, and the
//! history file is copied first so the browser's lock is never touched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};

const REFRESH: Duration = Duration::from_secs(30);

const BROWSERS: &[&str] = &[
    "chrome", "chromium", "brave", "firefox", "edge", "msedge", "vivaldi", "opera", "arc",
];

const TITLE_SUFFIXES: &[&str] = &[
    " - Google Chrome",
    " - Chromium",
    " - Brave",
    " - Vivaldi",
    " - Opera",
    " - Microsoft Edge",
    " - Microsoft\u{200b} Edge",
    " – Mozilla Firefox",
    " — Mozilla Firefox",
    " - Mozilla Firefox",
];

pub fn is_browser(app_name: &str) -> bool {
    let app = app_name.to_lowercase();
    BROWSERS.iter().any(|b| app.contains(b))
}

pub fn strip_suffix(title: &str) -> &str {
    TITLE_SUFFIXES
        .iter()
        .find_map(|s| title.strip_suffix(s))
        .unwrap_or(title)
}

#[derive(Clone, Copy)]
enum Kind {
    Chromium,
    Firefox,
}

struct HistoryDb {
    source: PathBuf,
    kind: Kind,
    copy: PathBuf,
    conn: Option<Connection>,
    refreshed: Option<Instant>,
}

impl HistoryDb {
    fn lookup(&mut self, title: &str) -> Option<String> {
        let stale = self.refreshed.is_none_or(|t| t.elapsed() > REFRESH);
        if stale {
            self.conn = None;
            std::fs::copy(&self.source, &self.copy).ok()?;
            self.conn =
                Connection::open_with_flags(&self.copy, OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
            self.refreshed = Some(Instant::now());
        }
        let conn = self.conn.as_ref()?;
        let (exact, prefix) = match self.kind {
            Kind::Chromium => (
                "SELECT url FROM urls WHERE title = ?1 ORDER BY last_visit_time DESC LIMIT 1",
                "SELECT url FROM urls WHERE title LIKE ?1 ORDER BY last_visit_time DESC LIMIT 1",
            ),
            Kind::Firefox => (
                "SELECT url FROM moz_places WHERE title = ?1 ORDER BY last_visit_date DESC LIMIT 1",
                "SELECT url FROM moz_places WHERE title LIKE ?1 ORDER BY last_visit_date DESC LIMIT 1",
            ),
        };
        let one = |sql: &str, arg: &str| -> Option<String> {
            conn.query_row(sql, params![arg], |r| r.get(0))
                .optional()
                .ok()
                .flatten()
        };
        let escaped = title.replace('%', "\\%").replace('_', "\\_");
        one(exact, title).or_else(|| {
            one(
                &prefix.replace("?1", "?1 ESCAPE '\\'"),
                &format!("{escaped}%"),
            )
        })
    }
}

pub struct BrowserUrls {
    dbs: Vec<HistoryDb>,
    cache: HashMap<String, (Option<String>, Instant)>,
}

impl BrowserUrls {
    pub fn new(temp_dir: &Path) -> Self {
        let _ = std::fs::create_dir_all(temp_dir);
        let dbs = history_files()
            .into_iter()
            .enumerate()
            .map(|(i, (source, kind))| HistoryDb {
                source,
                kind,
                copy: temp_dir.join(format!("history-{i}.sqlite")),
                conn: None,
                refreshed: None,
            })
            .collect();
        Self {
            dbs,
            cache: HashMap::new(),
        }
    }

    pub fn lookup(&mut self, app_name: &str, window_title: &str) -> Option<String> {
        if !is_browser(app_name) {
            return None;
        }
        let title = strip_suffix(window_title).trim();
        if title.chars().count() < 3 {
            return None;
        }
        if let Some((url, at)) = self.cache.get(title)
            && (url.is_some() || at.elapsed() < REFRESH)
        {
            return url.clone();
        }
        let url = self.dbs.iter_mut().find_map(|db| db.lookup(title));
        if self.cache.len() > 2000 {
            self.cache.clear();
        }
        self.cache
            .insert(title.to_string(), (url.clone(), Instant::now()));
        url
    }
}

fn history_files() -> Vec<(PathBuf, Kind)> {
    let mut out = Vec::new();
    let chromium_profiles: Vec<PathBuf> = {
        let mut p = Vec::new();
        #[cfg(target_os = "linux")]
        if let Some(c) = dirs::config_dir() {
            for d in [
                "google-chrome",
                "chromium",
                "BraveSoftware/Brave-Browser",
                "microsoft-edge",
                "vivaldi",
            ] {
                p.push(c.join(d));
            }
        }
        #[cfg(target_os = "macos")]
        if let Some(c) = dirs::config_dir() {
            for d in [
                "Google/Chrome",
                "Chromium",
                "BraveSoftware/Brave-Browser",
                "Microsoft Edge",
                "Vivaldi",
                "Arc/User Data",
            ] {
                p.push(c.join(d));
            }
        }
        #[cfg(windows)]
        if let Some(c) = dirs::data_local_dir() {
            for d in [
                "Google/Chrome/User Data",
                "Microsoft/Edge/User Data",
                "BraveSoftware/Brave-Browser/User Data",
                "Vivaldi/User Data",
            ] {
                p.push(c.join(d));
            }
        }
        p
    };
    for base in chromium_profiles {
        let history = base.join("Default").join("History");
        if history.exists() {
            out.push((history, Kind::Chromium));
        }
    }

    #[cfg(target_os = "linux")]
    let firefox = dirs::home_dir().map(|h| h.join(".mozilla/firefox"));
    #[cfg(target_os = "macos")]
    let firefox = dirs::config_dir().map(|c| c.join("Firefox/Profiles"));
    #[cfg(windows)]
    let firefox = dirs::config_dir().map(|c| c.join("Mozilla/Firefox/Profiles"));
    if let Some(dir) = firefox
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for e in entries.flatten() {
            let places = e.path().join("places.sqlite");
            if places.exists() {
                out.push((places, Kind::Firefox));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_and_browsers() {
        assert_eq!(
            strip_suffix("Inbox - Gmail - Google Chrome"),
            "Inbox - Gmail"
        );
        assert_eq!(strip_suffix("Docs — Mozilla Firefox"), "Docs");
        assert!(is_browser("Google Chrome"));
        assert!(is_browser("msedge"));
        assert!(!is_browser("Code"));
    }

    #[test]
    fn finds_url_in_chromium_history() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("History");
        let conn = Connection::open(&src).unwrap();
        conn.execute_batch(
            "CREATE TABLE urls (url TEXT, title TEXT, last_visit_time INTEGER);
             INSERT INTO urls VALUES ('https://old.example', 'Sprint board', 1);
             INSERT INTO urls VALUES ('https://jira.example/board', 'Sprint board', 2);
             INSERT INTO urls VALUES ('https://x.example', '100% done_ok', 3);",
        )
        .unwrap();
        drop(conn);
        let mut urls = BrowserUrls {
            dbs: vec![HistoryDb {
                source: src,
                kind: Kind::Chromium,
                copy: dir.path().join("copy.sqlite"),
                conn: None,
                refreshed: None,
            }],
            cache: HashMap::new(),
        };
        assert_eq!(
            urls.lookup("Google Chrome", "Sprint board - Google Chrome")
                .as_deref(),
            Some("https://jira.example/board")
        );
        assert_eq!(
            urls.lookup("Google Chrome", "100% done").as_deref(),
            Some("https://x.example")
        );
        assert_eq!(urls.lookup("Code", "Sprint board"), None);
    }
}
