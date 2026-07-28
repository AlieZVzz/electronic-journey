CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('macos', 'windows')),
    app_version TEXT NOT NULL,
    device_public_key BYTEA NOT NULL,
    revoked_at_utc TIMESTAMPTZ,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX devices_user_idx ON devices(user_id);

CREATE TABLE captures (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id UUID NOT NULL REFERENCES devices(id) ON DELETE RESTRICT,
    object_key TEXT NOT NULL UNIQUE,
    cipher_size BIGINT NOT NULL CHECK (cipher_size > 0),
    cipher_sha256 CHAR(64) NOT NULL,
    captured_at_utc TIMESTAMPTZ NOT NULL,
    encrypted_metadata BYTEA NOT NULL,
    object_state TEXT NOT NULL CHECK (
        object_state IN ('initialized', 'uploaded', 'verified', 'deleting', 'deleted')
    ),
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX captures_timeline_idx
    ON captures(user_id, captured_at_utc DESC, id DESC);

CREATE TABLE uploads (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    capture_id UUID NOT NULL UNIQUE REFERENCES captures(id) ON DELETE CASCADE,
    expires_at_utc TIMESTAMPTZ NOT NULL,
    completed_at_utc TIMESTAMPTZ,
    created_at_utc TIMESTAMPTZ NOT NULL DEFAULT now()
);
