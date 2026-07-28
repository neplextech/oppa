CREATE INDEX print_jobs_pending_idx
    ON print_jobs(state, received_at_ms);

CREATE INDEX print_jobs_updated_idx
    ON print_jobs(updated_at_ms);

CREATE INDEX printer_bindings_printer_idx
    ON printer_bindings(printer_id);
