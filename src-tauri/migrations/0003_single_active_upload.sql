UPDATE upload_items
SET state = 'failed', last_error_code = 'interrupted'
WHERE state IN ('pending', 'uploading');

UPDATE upload_batches
SET
    state = 'partial_failed',
    completed_items = (
        SELECT COUNT(*) FROM upload_items
        WHERE upload_items.batch_id = upload_batches.id
          AND upload_items.state = 'uploaded'
    ),
    failed_items = (
        SELECT COUNT(*) FROM upload_items
        WHERE upload_items.batch_id = upload_batches.id
          AND upload_items.state = 'failed'
    )
WHERE state IN ('pending', 'uploading');

CREATE UNIQUE INDEX upload_batches_single_active_idx
    ON upload_batches((1))
    WHERE state IN ('pending', 'uploading');
