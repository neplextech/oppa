# OPPA desktop application

OPPA is the Open Printer Proxy Agent desktop host. This workspace unit combines a compact React
interface with a narrow Tauri boundary around the shell-independent `oppa-agent` crate.

## Responsibilities

- account pairing and connection-state presentation
- printer, virtual printer, job, diagnostic, and background settings views
- system tray and close-to-background behavior
- translating typed frontend requests into agent operations

The application does not contain durable job logic, renderer or spooler implementations, server-side
queues, or product-specific printer routing.

## Commands

From the repository root:

```bash
pnpm --filter oppa dev
pnpm --filter oppa typecheck
OPPA_PRODUCT_DIR=products/default pnpm oppa:dev
OPPA_PRODUCT_DIR=products/default pnpm oppa:build
```

The plain Vite development command uses safe demonstration data when Tauri internals are absent.
The Tauri build uses the real Rust command service.

## Security boundary

The frontend receives only typed status, printer, job, and sanitized diagnostic objects. It is not
granted generic shell, filesystem, SQL, or network execution.

## Status

The initial application provides setup, overview, printer management, virtual output, job history,
diagnostics, settings, tray behavior, and start-on-login integration. Physical behavior still
depends on the selected platform and installed printer backend.
