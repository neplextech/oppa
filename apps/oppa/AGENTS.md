# OPPA desktop guidance

- Treat Tauri as a host for `oppa-agent`; do not move core lifecycle
  or printer logic into commands.
- Frontend commands and events must use explicit request and response
  types.
- Never expose generic filesystem, shell, SQL, or network execution.
- Keep the interface compact: setup, overview, printers, virtual
  printers, jobs, diagnostics, and settings.
- Closing the main window hides it while the tray agent continues;
  only the explicit Quit action stops the process.
- Display `submitted` rather than `printed` unless a backend can
  verify physical output.
- Diagnostics are sanitized and bounded.
- Run both `pnpm --filter oppa typecheck` and `cargo test -p oppa`
  after boundary changes.
