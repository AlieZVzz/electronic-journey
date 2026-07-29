PRAGMA foreign_keys = ON;

CREATE TABLE captures (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL,
    display_id TEXT NOT NULL,
    captured_at_utc TEXT NOT NULL,
    timezone TEXT NOT NULL,
    local_path TEXT NOT NULL UNIQUE,
    thumbnail_path TEXT UNIQUE,
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    content_sha256 TEXT NOT NULL,
    thumbnail_state TEXT NOT NULL CHECK (
        thumbnail_state IN ('ready', 'pending', 'failed')
    ),
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    created_at_utc TEXT NOT NULL
);

CREATE INDEX captures_captured_at_idx ON captures(captured_at_utc DESC);
CREATE INDEX captures_thumbnail_state_idx ON captures(thumbnail_state);

CREATE TABLE ai_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    question TEXT NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'running', 'completed', 'failed', 'cancelled')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error_code TEXT,
    response_text TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);

CREATE INDEX ai_jobs_state_idx ON ai_jobs(state, created_at_utc);

CREATE TABLE ai_job_captures (
    job_id TEXT NOT NULL REFERENCES ai_jobs(id) ON DELETE CASCADE,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE RESTRICT,
    ordinal INTEGER NOT NULL CHECK (ordinal >= 0),
    PRIMARY KEY (job_id, capture_id),
    UNIQUE (job_id, ordinal)
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);
