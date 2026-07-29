PRAGMA foreign_keys = ON;

DROP TABLE ai_job_captures;
DROP TABLE ai_jobs;

CREATE TABLE remote_profiles (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username TEXT NOT NULL,
    private_key_path TEXT NOT NULL,
    host_key_fingerprint TEXT NOT NULL,
    remote_root TEXT NOT NULL,
    has_passphrase INTEGER NOT NULL DEFAULT 0 CHECK (has_passphrase IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);

CREATE TABLE upload_batches (
    id TEXT PRIMARY KEY NOT NULL,
    profile_id TEXT NOT NULL REFERENCES remote_profiles(id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'uploading', 'completed', 'partial_failed', 'cancelled')
    ),
    total_items INTEGER NOT NULL CHECK (total_items > 0),
    total_bytes INTEGER NOT NULL CHECK (total_bytes >= 0),
    completed_items INTEGER NOT NULL DEFAULT 0 CHECK (completed_items >= 0),
    failed_items INTEGER NOT NULL DEFAULT 0 CHECK (failed_items >= 0),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);

CREATE INDEX upload_batches_state_idx
    ON upload_batches(state, created_at_utc);

CREATE TABLE upload_items (
    id TEXT PRIMARY KEY NOT NULL,
    batch_id TEXT NOT NULL REFERENCES upload_batches(id) ON DELETE CASCADE,
    capture_id TEXT NOT NULL,
    remote_path TEXT NOT NULL,
    file_size INTEGER NOT NULL CHECK (file_size >= 0),
    content_sha256 TEXT NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('pending', 'uploading', 'uploaded', 'failed', 'cancelled')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_error_code TEXT,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (batch_id, capture_id)
);

CREATE INDEX upload_items_capture_idx
    ON upload_items(capture_id, created_at_utc DESC);
CREATE INDEX upload_items_state_idx
    ON upload_items(state, created_at_utc);
