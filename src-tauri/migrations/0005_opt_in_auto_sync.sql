ALTER TABLE remote_profiles
ADD COLUMN auto_sync_enabled INTEGER NOT NULL DEFAULT 0
    CHECK (auto_sync_enabled IN (0, 1));

ALTER TABLE remote_profiles
ADD COLUMN sync_interval_minutes INTEGER NOT NULL DEFAULT 30
    CHECK (sync_interval_minutes IN (15, 30, 60, 120, 240));

ALTER TABLE remote_profiles
ADD COLUMN next_auto_sync_at_utc TEXT;

ALTER TABLE remote_profiles
ADD COLUMN last_auto_sync_attempt_at_utc TEXT;

ALTER TABLE remote_profiles
ADD COLUMN last_auto_sync_state TEXT
    CHECK (
        last_auto_sync_state IS NULL OR
        last_auto_sync_state IN (
            'running',
            'completed',
            'partial_failed',
            'empty',
            'skipped_busy',
            'suspended'
        )
    );

ALTER TABLE remote_profiles
ADD COLUMN last_auto_sync_completed_items INTEGER NOT NULL DEFAULT 0
    CHECK (last_auto_sync_completed_items >= 0);

ALTER TABLE remote_profiles
ADD COLUMN last_auto_sync_failed_items INTEGER NOT NULL DEFAULT 0
    CHECK (last_auto_sync_failed_items >= 0);

ALTER TABLE remote_profiles
ADD COLUMN auto_sync_suspended_reason TEXT;

ALTER TABLE upload_batches
ADD COLUMN source TEXT NOT NULL DEFAULT 'manual'
    CHECK (source IN ('manual', 'automatic'));

CREATE INDEX upload_batches_source_created_idx
    ON upload_batches(source, created_at_utc DESC);
