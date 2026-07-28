PRAGMA foreign_keys = ON;

CREATE TABLE captures (
    id TEXT PRIMARY KEY NOT NULL,
    device_id TEXT NOT NULL,
    display_id TEXT NOT NULL,
    captured_at_utc TEXT NOT NULL,
    timezone TEXT NOT NULL,
    local_path TEXT NOT NULL UNIQUE,
    cipher_size INTEGER NOT NULL CHECK (cipher_size >= 0),
    cipher_sha256 TEXT NOT NULL,
    key_version INTEGER NOT NULL CHECK (key_version > 0),
    sync_state TEXT NOT NULL CHECK (
        sync_state IN ('local_only', 'pending', 'uploading', 'retry', 'completed', 'failed')
    ),
    favorite INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    created_at_utc TEXT NOT NULL
);

CREATE INDEX captures_captured_at_idx ON captures(captured_at_utc DESC);
CREATE INDEX captures_sync_state_idx ON captures(sync_state);

CREATE TABLE upload_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    capture_id TEXT NOT NULL UNIQUE REFERENCES captures(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'uploading', 'retry', 'completed', 'failed')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at_utc TEXT,
    last_error_code TEXT,
    lease_expires_at_utc TEXT,
    updated_at_utc TEXT NOT NULL
);

CREATE INDEX upload_jobs_ready_idx
    ON upload_jobs(state, next_attempt_at_utc);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);
