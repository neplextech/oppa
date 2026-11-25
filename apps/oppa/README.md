# OPPA desktop application

OPPA is the Open Printer Proxy Agent desktop host. This workspace unit combines a compact React
interface with a narrow Tauri boundary around the shell-independent `oppa-agent` crate.

## Responsibilities

- server discovery, pairing, and explicit connection-state presentation
- printer, virtual printer, job, diagnostic, and background settings views
- validated runtime selection of one OpenPrinter server base URL
- signed stable-update checks and installation through the Tauri updater
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

## PDF test prints

The printer details panel shows which PDF route a printer uses, the paper sizes
it offers, and whether custom media and duplex are available. The PDF test form
selects a file of up to 20 MB and sends it once over local IPC. The agent
validates the file and submits it as a local test, without storing it in the
job ledger or sending it to a server. Duplex and custom sizes appear only when
the printer reports them.

## Security boundary

The frontend receives only typed status, printer, job, and sanitized diagnostic objects. It is not
granted generic shell, filesystem, SQL, or network execution.

The OpenPrinter server base URL and paired public identifiers are non-secret settings. The private
Ed25519 key remains in operating-system secure storage and never enters SQLite or the frontend.
Discovery supplies the current pairing and gateway endpoints. Plain HTTP is accepted only for
loopback development; production discovery/pairing requires HTTPS and the gateway resolves to WSS.
Changing or forgetting the service deletes the local key and requires a new pairing code.

Update artifacts are fetched from GitHub Releases and verified by the native Tauri updater. Replace
the updater public-key placeholder in `src-tauri/tauri.conf.json` before publishing a release.

## Status

The initial application provides setup, overview, printer management, virtual output, job history,
diagnostics, settings, tray behavior, and start-on-login integration. Physical behavior still
depends on the selected platform and installed printer backend.

## Onboarding and design foundation

Setup uses three exclusive stages: server discovery, explicit identity verification and pairing,
then printer authorization. Server URLs start from the embedded product configuration. Recent
servers and validated deep links use the same flow; changing an existing pairing requires a
confirmation before its credential is removed by the existing agent operation.

Only the stage, server address, and computer name persist as view preferences. Pairing codes remain
in memory. Resuming pairing re-discovers the service identity. Newly paired computers start with
printer access disabled until the user selects printers. Step 3 uses the existing typed printer
configuration operation; printer delivery and protocol semantics are unchanged.

The shared foundation defines light/dark surfaces, 13 px body text, 12 px secondary text, 36 px
inputs, 6 px control radii, and 10 px dialogs. The onboarding title bar uses a minimal mode of the
existing native-aware chrome. The remaining screens use the same foundation; see the desktop review below.

See [the phase 1 and 2 design review](design-review/README.md) for screenshots, reference interfaces,
validation evidence, and the development-only visual fixture.

## Operational desktop workspace

The shell, overview, device registry, delivery history, virtual capture workspace, preferences, logs,
and tray use one compact visual language. See [the phases 3–9 desktop review](design-review/desktop/README.md)
for rendered screenshots, interaction checks, narrow native changes, and verification limits.

Inspector dividers support pointer dragging and arrow keys. Sidebar collapse state, selections, and
inspector widths persist as local UI preferences. At narrow widths an inspector temporarily replaces
its list. Contextual commands use Cmd+K on macOS and Ctrl+K elsewhere.

Virtual output remains developer-only and product-capability gated. Existing raster captures expose
bounded multipage previews, fit/zoom/pan, local export, and rendering information. Original binary
payloads remain private; diagnostic export strips all captured history. Native webview download
behavior and physical printing require separate platform acceptance testing.
