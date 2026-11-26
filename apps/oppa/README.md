# OPPA application

OPPA is the Open Printer Proxy Agent host for Windows, macOS, Linux, Android, and iOS. This
workspace unit combines a compact React interface with a narrow Tauri boundary around the
shell-independent `oppa-agent` crate. All platforms share one frontend, one Rust host, and one
agent; platform differences are selected at compile time and reported at runtime as
`AgentStatus.capabilities`.

## Responsibilities

- server discovery, pairing, and explicit connection-state presentation
- printer, virtual printer, job, diagnostic, and background settings views
- validated runtime selection of one OpenPrinter server base URL
- signed stable-update checks and installation through the Tauri updater (desktop), or verified
  direct-download APK updates approved by the user (Android)
- system tray and close-to-background behavior (desktop)
- the Android foreground service, iOS keep-awake and bounded background time, lifecycle-driven
  reconnects, and DNS-SD network printer discovery (mobile)
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

Mobile, from the repository root (see `apps/www/content/docs/mobile.mdx`):

```bash
pnpm oppa:android:dev
pnpm oppa:android:build --debug --apk
pnpm oppa:ios:init && pnpm oppa:ios:brand && pnpm oppa:ios:dev
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

## Host structure

| Module | Role |
| --- | --- |
| `service.rs` | `AgentService`: process-owned runtime shared by every platform |
| `runtime_owner.rs` | one runtime and one connection supervisor per process |
| `platform.rs` | capability table per platform; product features are intersected with it |
| `desktop.rs` | tray, application menu, close-to-background (desktop only) |
| `mobile.rs` | sandbox paths, credential store, lifecycle loop, notification status (mobile only) |
| `lifecycle.rs` | pure reconnect/suspension/stop policy, tested on every host |
| `network_discovery.rs` | DNS-SD result normalization into add-printer suggestions |
| `updates.rs` | Android update manifest and download verification |

Native Android and iOS code lives in `../plugins/mobile` and contains no agent logic.

## Security boundary

The frontend receives only typed status, printer, job, and sanitized diagnostic objects. It is not
granted generic shell, filesystem, SQL, or network execution.

The OpenPrinter server base URL and paired public identifiers are non-secret settings. The private
Ed25519 key remains in operating-system secure storage (Keychain, Credential Manager, Secret
Service, Android Keystore-backed encryption, or the iOS Keychain) and never enters SQLite or the
frontend.
Discovery supplies the current pairing and gateway endpoints. Plain HTTP is accepted only for
loopback development; production discovery/pairing requires HTTPS and the gateway resolves to WSS.
Changing or forgetting the service deletes the local key and requires a new pairing code.

Release builds discover updates through the Neplex update server, which serves the artifacts published
to GitHub Releases:

- desktop: `https://updates.neplex.com.np/oppa/stable/tauri/{{target}}/{{arch}}/{{current_version}}`,
  verified by the native Tauri updater against the public key in `src-tauri/tauri.conf.json`;
- Android: `https://updates.neplex.com.np/oppa/stable/android/latest.json`, overridable at compile time
  with `OPPA_ANDROID_UPDATE_MANIFEST`.

HTTP 204 from either endpoint means no update is available. Development builds disable update checks.

## Status

Android: built and exercised on an Android 13 emulator (pairing, deep links, foreground service,
Doze, activity recreation, process death recovery, updates over an existing install, raw TCP and
virtual printing, PDF). iOS: implemented; simulator builds run in CI on macOS and need Xcode
locally. Physical-device checks are listed in the mobile documentation.


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

### Thermal paper inspection

Thermal captures default to actual size (100% at CSS physical-unit scale).
Receipt mode adds a warm paper surface, a restrained shadow, and presentation-only
feed/cut markers, with equal 16 px presentation padding outside the captured
raster on every side. The bitmap retains its physical scale; displayed dimensions
describe the captured raster, not this decorative framing. A serrated lower edge indicates a recorded cut request, not
verified hardware cutting. Raw raster removes all paper decoration. Both modes
export the same full-resolution source PNG within the existing capture budget.
Fit window only shrinks thermal paper; Fit width and manual zoom can enlarge it.
Office/PDF previews keep their page fitting and multipage navigation.

The isolated `test/receipt.browser.mjs` acceptance runner uses PNG fixtures from
the Rust ESC/POS interpreter. Set `OPPA_PLAYWRIGHT_MODULE` to an external Playwright
installation and run it against the Vite development server. No fixture content
or browser automation dependency is included in the production application.
