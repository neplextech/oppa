# Agent crate guidance

- Keep this crate independent of Tauri and any particular desktop
  shell.
- Persist a validated job before sending `job-received`.
- Keep rendering and spooler submission behind their existing
  boundaries.
- Treat delivery as at least once and enforce durable idempotency.
- Restore pending jobs during startup and report uncertain outcomes
  honestly.
- Expose bounded, sanitized state events to hosts; never expose
  credentials or raw storage handles.
- Use the virtual spooler for lifecycle tests.
