CREATE TABLE printers (
    printer_id TEXT PRIMARY KEY NOT NULL,
    printer_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE printer_bindings (
    binding_id TEXT PRIMARY KEY NOT NULL,
    printer_id TEXT NOT NULL REFERENCES printers(printer_id) ON DELETE CASCADE,
    metadata_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE print_jobs (
    job_id TEXT PRIMARY KEY NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    printer_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (
        state IN ('received', 'submitting', 'submitted', 'failed', 'cancelled')
    ),
    received_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    retry_attempts INTEGER NOT NULL DEFAULT 0 CHECK (retry_attempts >= 0),
    recovery_count INTEGER NOT NULL DEFAULT 0 CHECK (recovery_count >= 0),
    receipt_json TEXT,
    error_json TEXT
);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE connection_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE diagnostics_metadata (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at_ms INTEGER NOT NULL
);
