ALTER TABLE captures
ADD COLUMN stable_content_sha256 TEXT;

ALTER TABLE captures
ADD COLUMN comparison_policy TEXT;
