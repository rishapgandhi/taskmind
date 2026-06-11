"""SQLite database module - schema creation and CRUD operations."""
import sqlite3
from datetime import datetime, date
from taskmind.config import DB_PATH, ensure_dirs

SCHEMA = """
CREATE TABLE IF NOT EXISTS activities (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    window_title TEXT,
    app_name TEXT,
    window_class TEXT,
    project_name TEXT,
    duration_seconds INTEGER DEFAULT 10,
    is_idle INTEGER DEFAULT 0,
    browser_url TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS timesheet_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    project_name TEXT,
    start_time TEXT NOT NULL,
    end_time TEXT NOT NULL,
    duration_minutes INTEGER NOT NULL,
    description TEXT,
    status TEXT DEFAULT 'draft'
);

CREATE TABLE IF NOT EXISTS daily_recaps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL UNIQUE,
    total_tracked_minutes INTEGER,
    total_idle_minutes INTEGER,
    summary_text TEXT,
    project_breakdown TEXT,
    created_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS activities_fts USING fts5(
    window_title, app_name, project_name,
    content=activities, content_rowid=id
);

CREATE VIRTUAL TABLE IF NOT EXISTS transcripts_fts USING fts5(
    transcript,
    content=recordings,
    content_rowid=id
);

CREATE TABLE IF NOT EXISTS recordings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_seconds INTEGER,
    file_path TEXT,
    transcript TEXT,
    status TEXT DEFAULT 'recording',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS git_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    repo_path TEXT NOT NULL,
    event_type TEXT NOT NULL,
    branch TEXT,
    message TEXT,
    files_changed INTEGER DEFAULT 0,
    project_id INTEGER
);

CREATE TRIGGER IF NOT EXISTS activities_ai AFTER INSERT ON activities BEGIN
    INSERT INTO activities_fts(rowid, window_title, app_name, project_name)
    VALUES (new.id, new.window_title, new.app_name, new.project_name);
END;

CREATE TRIGGER IF NOT EXISTS recordings_ai AFTER UPDATE OF transcript ON recordings
WHEN new.transcript IS NOT NULL BEGIN
    INSERT OR REPLACE INTO transcripts_fts(rowid, transcript)
    VALUES (new.id, new.transcript);
END;
"""


def get_db():
    """Get database connection."""
    ensure_dirs()
    conn = sqlite3.connect(DB_PATH)
    conn.row_factory = sqlite3.Row
    return conn


def init_db():
    """Initialize database schema."""
    conn = get_db()
    conn.executescript(SCHEMA)
    # Migration: add browser_url column if missing
    cols = [r[1] for r in conn.execute("PRAGMA table_info(activities)").fetchall()]
    if "browser_url" not in cols:
        conn.execute("ALTER TABLE activities ADD COLUMN browser_url TEXT DEFAULT ''")
    conn.commit()
    conn.close()


def insert_activity(timestamp, window_title, app_name, window_class, project_name, duration, is_idle=False, browser_url=""):
    """Insert an activity record."""
    conn = get_db()
    conn.execute(
        "INSERT INTO activities (timestamp, window_title, app_name, window_class, project_name, duration_seconds, is_idle, browser_url) VALUES (?,?,?,?,?,?,?,?)",
        (timestamp, window_title, app_name, window_class, project_name, duration, int(is_idle), browser_url),
    )
    conn.commit()
    conn.close()


def get_activities_for_date(target_date=None):
    """Get all activities for a given date."""
    if target_date is None:
        target_date = date.today().isoformat()
    conn = get_db()
    rows = conn.execute(
        "SELECT * FROM activities WHERE timestamp LIKE ? ORDER BY timestamp",
        (target_date + "%",),
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def get_project_summary(target_date=None):
    """Get total seconds per project for a date."""
    if target_date is None:
        target_date = date.today().isoformat()
    conn = get_db()
    rows = conn.execute(
        "SELECT project_name, SUM(duration_seconds) as total_seconds, COUNT(*) as count "
        "FROM activities WHERE timestamp LIKE ? AND is_idle=0 "
        "GROUP BY project_name ORDER BY total_seconds DESC",
        (target_date + "%",),
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def save_timesheet_entries(entries):
    """Save generated timesheet entries."""
    conn = get_db()
    conn.executemany(
        "INSERT INTO timesheet_entries (date, project_name, start_time, end_time, duration_minutes, description, status) VALUES (?,?,?,?,?,?,?)",
        [(e["date"], e["project_name"], e["start_time"], e["end_time"], e["duration_minutes"], e["description"], "draft") for e in entries],
    )
    conn.commit()
    conn.close()


def get_timesheet_for_date(target_date=None):
    """Get timesheet entries for a date."""
    if target_date is None:
        target_date = date.today().isoformat()
    conn = get_db()
    rows = conn.execute(
        "SELECT * FROM timesheet_entries WHERE date=? ORDER BY start_time",
        (target_date,),
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def search_activities(query):
    """Full-text search across activities."""
    conn = get_db()
    rows = conn.execute(
        "SELECT a.* FROM activities a JOIN activities_fts f ON a.id=f.rowid WHERE activities_fts MATCH ? ORDER BY a.timestamp DESC LIMIT 50",
        (query,),
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def get_total_tracked_today():
    """Get total tracked seconds today (non-idle)."""
    today = date.today().isoformat()
    conn = get_db()
    row = conn.execute(
        "SELECT COALESCE(SUM(duration_seconds),0) as total FROM activities WHERE timestamp LIKE ? AND is_idle=0",
        (today + "%",),
    ).fetchone()
    conn.close()
    return row["total"]


def insert_recording(started_at, file_path):
    """Insert a new recording entry."""
    conn = get_db()
    conn.execute(
        "INSERT INTO recordings (started_at, file_path, status, created_at) VALUES (?,?,?,?)",
        (started_at, file_path, "recording", started_at),
    )
    conn.commit()
    conn.close()


def update_recording(file_path, ended_at, duration, transcript, status="done"):
    """Update recording with transcript after completion."""
    conn = get_db()
    conn.execute(
        "UPDATE recordings SET ended_at=?, duration_seconds=?, transcript=?, status=? WHERE file_path=?",
        (ended_at, duration, transcript, status, file_path),
    )
    conn.commit()
    conn.close()


def get_recordings(limit=20):
    """Get recent recordings."""
    conn = get_db()
    rows = conn.execute(
        "SELECT * FROM recordings ORDER BY started_at DESC LIMIT ?", (limit,)
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def search_transcripts(query):
    """Search transcripts via FTS5."""
    conn = get_db()
    rows = conn.execute(
        "SELECT r.* FROM recordings r JOIN transcripts_fts f ON r.id=f.rowid "
        "WHERE transcripts_fts MATCH ? ORDER BY r.started_at DESC LIMIT 20",
        (query,),
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]
