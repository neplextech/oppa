ALTER TABLE print_jobs
    ADD COLUMN source_message_id TEXT NOT NULL DEFAULT '';

ALTER TABLE print_jobs
    ADD COLUMN payload_sha256 TEXT NOT NULL DEFAULT '';

-- Normalize the initial agent payload wrapper. The source message identity is
-- delivery metadata, not print-job content, so it must not affect duplicate
-- comparison.
UPDATE print_jobs
SET source_message_id = json_extract(payload_json, '$.correlationId'),
    payload_json = json_extract(payload_json, '$.job')
WHERE json_type(payload_json, '$.job') = 'object'
  AND json_type(payload_json, '$.correlationId') = 'text';

CREATE INDEX print_jobs_terminal_retention_idx
    ON print_jobs(state, updated_at_ms DESC, job_id DESC);

CREATE TABLE outbound_status_outbox (
    message_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES print_jobs(job_id) ON DELETE CASCADE,
    payload_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX outbound_status_outbox_created_idx
    ON outbound_status_outbox(created_at_ms, message_id);
