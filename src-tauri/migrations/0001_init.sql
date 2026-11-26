-- TaskMind v2 initial schema. Never edit after release: add 0002_*.sql instead.
-- All timestamps are UTC ISO 8601 strings.

CREATE TABLE projects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    color       TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE project_rules (
    id          INTEGER PRIMARY KEY,
    project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind        TEXT NOT NULL CHECK (kind IN ('window_title','app_name','git_branch','directory','url')),
    pattern     TEXT NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE activities (
    id            INTEGER PRIMARY KEY,
    started_at    TEXT NOT NULL,
    ended_at      TEXT NOT NULL,
    app_name      TEXT,
    window_title  TEXT,
    url           TEXT,
    project_id    INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    is_idle       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_activities_started_at ON activities(started_at);

CREATE TABLE git_events (
    id             INTEGER PRIMARY KEY,
    at             TEXT NOT NULL,
    repo_path      TEXT NOT NULL,
    kind           TEXT NOT NULL,
    branch         TEXT,
    message        TEXT,
    files_changed  INTEGER NOT NULL DEFAULT 0,
    project_id     INTEGER REFERENCES projects(id) ON DELETE SET NULL
);
CREATE INDEX idx_git_events_at ON git_events(at);

CREATE TABLE meetings (
    id           INTEGER PRIMARY KEY,
    title        TEXT,
    started_at   TEXT NOT NULL,
    ended_at     TEXT,
    duration_s   INTEGER,
    audio_path   TEXT,
    audio_bytes  INTEGER,
    source_app   TEXT,
    status       TEXT NOT NULL DEFAULT 'recording'
                 CHECK (status IN ('recording','finalizing','queued','transcribing','done','failed')),
    project_id   INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_meetings_started_at ON meetings(started_at);

CREATE TABLE transcript_segments (
    id          INTEGER PRIMARY KEY,
    meeting_id  INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    start_ms    INTEGER NOT NULL,
    end_ms      INTEGER NOT NULL,
    speaker     TEXT NOT NULL DEFAULT 'others',
    text        TEXT NOT NULL
);
CREATE INDEX idx_segments_meeting ON transcript_segments(meeting_id, start_ms);

CREATE TABLE meeting_notes (
    meeting_id         INTEGER PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
    summary            TEXT,
    decisions_json     TEXT,
    action_items_json  TEXT,
    provider           TEXT,
    model              TEXT,
    generated_at       TEXT,
    edited             INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE timesheet_entries (
    id           INTEGER PRIMARY KEY,
    date         TEXT NOT NULL,
    project_id   INTEGER REFERENCES projects(id) ON DELETE SET NULL,
    start_at     TEXT NOT NULL,
    end_at       TEXT NOT NULL,
    minutes      INTEGER NOT NULL,
    description  TEXT,
    status       TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','approved'))
);
CREATE INDEX idx_timesheet_date ON timesheet_entries(date);

CREATE TABLE daily_recaps (
    date          TEXT PRIMARY KEY,
    summary       TEXT,
    data_json     TEXT,
    generated_at  TEXT NOT NULL
);

CREATE TABLE uploads (
    id           INTEGER PRIMARY KEY,
    meeting_id   INTEGER NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    target       TEXT NOT NULL,
    remote_id    TEXT,
    status       TEXT NOT NULL DEFAULT 'pending',
    uploaded_at  TEXT
);

CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value_json  TEXT NOT NULL
);

-- Full-text search (external content tables kept in sync by triggers)
CREATE VIRTUAL TABLE activities_fts USING fts5(
    window_title, app_name, url, content='activities', content_rowid='id'
);
CREATE TRIGGER activities_ai AFTER INSERT ON activities BEGIN
    INSERT INTO activities_fts(rowid, window_title, app_name, url)
    VALUES (new.id, new.window_title, new.app_name, new.url);
END;
CREATE TRIGGER activities_ad AFTER DELETE ON activities BEGIN
    INSERT INTO activities_fts(activities_fts, rowid, window_title, app_name, url)
    VALUES ('delete', old.id, old.window_title, old.app_name, old.url);
END;
CREATE TRIGGER activities_au AFTER UPDATE ON activities BEGIN
    INSERT INTO activities_fts(activities_fts, rowid, window_title, app_name, url)
    VALUES ('delete', old.id, old.window_title, old.app_name, old.url);
    INSERT INTO activities_fts(rowid, window_title, app_name, url)
    VALUES (new.id, new.window_title, new.app_name, new.url);
END;

CREATE VIRTUAL TABLE segments_fts USING fts5(
    text, content='transcript_segments', content_rowid='id'
);
CREATE TRIGGER segments_ai AFTER INSERT ON transcript_segments BEGIN
    INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER segments_ad AFTER DELETE ON transcript_segments BEGIN
    INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER segments_au AFTER UPDATE ON transcript_segments BEGIN
    INSERT INTO segments_fts(segments_fts, rowid, text) VALUES ('delete', old.id, old.text);
    INSERT INTO segments_fts(rowid, text) VALUES (new.id, new.text);
END;
