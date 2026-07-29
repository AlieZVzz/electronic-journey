ALTER TABLE captures
ADD COLUMN pixel_sha256 TEXT;

CREATE INDEX captures_latest_display_idx
    ON captures(device_id, display_id, captured_at_utc DESC, id DESC);
