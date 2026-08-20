PRAGMA foreign_keys = ON;

CREATE INDEX captures_favorite_time_idx
ON captures(favorite, captured_at_utc DESC, id DESC);

CREATE TABLE tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL UNIQUE,
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL
);

CREATE TABLE capture_tags (
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at_utc TEXT NOT NULL,
    PRIMARY KEY (capture_id, tag_id)
);

CREATE INDEX capture_tags_tag_capture_idx
ON capture_tags(tag_id, capture_id);

CREATE TABLE privacy_app_rules (
    id TEXT PRIMARY KEY NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('macos', 'windows')),
    app_identifier TEXT NOT NULL,
    display_name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at_utc TEXT NOT NULL,
    updated_at_utc TEXT NOT NULL,
    UNIQUE (platform, app_identifier)
);

CREATE INDEX privacy_app_rules_enabled_idx
ON privacy_app_rules(enabled, platform);
