# OPPA desktop redesign: phases 3–9

The approved onboarding palette, typography, and control geometry are retained. The user's subsequent
reference image guides the centered title bar, neutral sidebar selection, developer-tool grouping,
flat overview sections, and restrained status treatments. The application remains a background utility.

## Implemented phases

| Phase | Result |
| --- | --- |
| 3 · Shell | Minimal native-aware chrome, 208 px sidebar, persisted icon-only mode, accessible tooltips, platform shortcut labels, and a 28 px status bar. |
| 4 · Overview | Readiness and recovery states, enabled-printer availability, pending jobs, retained submission counts, direct device/job inspection, and compact activity. |
| 5 · Printers | Search and access/availability filters, keyboard navigation, context menus, persistent selection, resizable inspector, explicit access/mode controls, PDF settings, local tests, and a validated network-printer dialog. |
| 6 · Print jobs | Search, state filters, sorting, retained selection, accurate lifecycle observations, copyable identifiers, failure detail, and confirmed clearing through overflow. |
| 7 · Virtual printing | Capture history, thermal/office canvas, pan/zoom/fit, bounded multipage navigation, captured-output export, optional inspector, separate simulation/sound controls, and creation/deletion. |
| 8 · Preferences/logs | General, Connection, Appearance, Developer, and About categories; flat preference rows; confirmed credential changes; searchable severity-filtered logs; expandable sanitized details and system/provider information. |
| 9 · Native/polish | Event-driven native tray status, connection icon overlay, service identity, pending/availability counts, navigation/reconnect, explicit hide versus quit, contextual commands, focus management, controlled selection, and responsive inspectors. |

The collapsed sidebar keeps the same navigation order as the expanded sidebar. Settings exists only
at the bottom. A redundant server shortcut that opened Settings above Overview was removed after review.

Interface labels, rows, buttons, and canvases do not select like webpage text. Inputs, technical values,
errors, raw projections, and expanded diagnostic messages remain selectable and copyable.

## Screenshots

These are actual React application renders using an isolated development fixture. Both 800×600 and
1280×800 captures are saved; the layout acceptance runner also checks 1024×768 and 1600×1000.

| Screen | Light | Dark |
| --- | --- | --- |
| Overview | ![Overview light](overview-light-1280x800.png) | ![Overview dark](overview-dark-1280x800.png) |
| Printer list | ![Printers light](printers-light-1280x800.png) | ![Printers dark](printers-dark-1280x800.png) |
| Printer inspector | ![Printer inspector light](printer-inspector-light-1280x800.png) | ![Printer inspector dark](printer-inspector-dark-1280x800.png) |
| Print jobs | ![Jobs light](jobs-light-1280x800.png) | ![Jobs dark](jobs-dark-1280x800.png) |
| Job inspector | ![Job inspector light](job-inspector-light-1280x800.png) | ![Job inspector dark](job-inspector-dark-1280x800.png) |
| Thermal workspace | ![Thermal light](thermal-light-1280x800.png) | ![Thermal dark](thermal-dark-1280x800.png) |
| Office workspace | ![Office light](office-light-1280x800.png) | ![Office dark](office-dark-1280x800.png) |
| Settings | ![Settings light](settings-light-1280x800.png) | ![Settings dark](settings-dark-1280x800.png) |
| Connection preferences | ![Connection light](connection-light-1280x800.png) | ![Connection dark](connection-dark-1280x800.png) |
| Appearance preferences | ![Appearance light](appearance-light-1280x800.png) | ![Appearance dark](appearance-dark-1280x800.png) |
| Logs | ![Logs light](logs-light-1280x800.png) | ![Logs dark](logs-dark-1280x800.png) |

Additional inspected states:

- [Collapsed light](collapsed-light.png), [collapsed dark](collapsed-dark.png)
- [Network printer dialog](add-printer-dark.png), [contextual commands](commands-light.png)
- [Simulation controls](simulation-dark.png), [clear-record confirmation](clear-records-dark.png)
- [Unavailable printers](unavailable.png), [reconnecting server](disconnected.png)
- [Empty device list](empty-printers.png), [expanded error log](expanded-log.png)
- [Office inspector](office-inspector.png)
- [Small printer inspector](printer-inspector-light-800x600.png), [small thermal canvas](thermal-dark-800x600.png)

## Native and boundary changes

No wire protocol, durable job state, retry policy, credential storage, or spooler behavior changed.

The desktop status projection adds the existing compiled virtual-printer capability, allowing the
frontend to hide and gate developer controls correctly. The virtual capture projection exposes existing
raster-page previews and source dimensions for navigation. It is capped at 16 previews and 4 MiB of
encoded PNG input per captured output; total captured-page count remains available. This is a local
IPC projection, not a new rendering or document-delivery feature. Preview payloads remain excluded
from diagnostic export because virtual histories are stripped before serialization.

Tray updates follow existing agent events and use lightweight catalog counts without rendering
captures. A small status overlay preserves the product icon. Native menus expose Overview, Printers,
Print jobs, Settings, Logs, reconnect, hiding, and explicit shutdown. No custom tray window, periodic
telemetry, new notifications, or application dependencies were added.

Frontend state events are briefly coalesced to avoid redundant capture projections. Native structured
error messages are normalized into readable frontend errors. Printer/queue selection and inspector
widths are local UI preferences. Narrow inspectors replace their lists and restore focus on closing.

## Evidence and limits

- Workspace formatting, ESLint, TypeScript checks, Clippy with all targets/features, JavaScript suites,
  Rust workspace tests, and the OPPA production frontend build passed.
- The Rust desktop host has 22 passing tests, including new tray-icon and bounded-page-projection checks.
- Desktop browser acceptance passed 88 layouts (11 screen variants × two themes × four sizes),
  plus dialog, navigation, resizing, selection, developer-gate, failure/empty-state, and page-navigation checks.
- Onboarding acceptance passed its 24 layouts and original pairing/recovery interactions. Its body
  layout is unchanged; the shared title bar now uses centered minimal branding to follow the supplied reference.
- The build retains Vite's single-chunk size advisory. No dependency was added to the application.
- Browser rendering and mock agent interactions do not prove live Tauri window controls, native tray
  menu rendering, OS deep-link registration, secure-storage behavior, or physical printer acceptance.
- Output export uses webview download handling and needs native-platform save-behavior acceptance.
- Full original binary payloads are not exposed by the inspector. Exceeding the preview budget shows
  only the available pages with an explicit count. Actual screen-size calibration is not claimed.
- An exact "submitted today" total is omitted: retained records expose receipt and latest-update times,
  not a reliable separate submission timestamp. The UI labels its count as retained submitted records.

Rust commands used the installed `1.99.1` toolchain because this machine's `stable` alias cannot run
`rustc`/`rustfmt`. Cross-platform native behavior remains a runtime acceptance boundary.

## Reproduction

```bash
pnpm --filter oppa dev --host 127.0.0.1
pnpm --filter oppa test
```

Open `/test/desktop.html?theme=dark#/overview`. Fixture flags include `collapsed`, `empty`, `long`,
`disconnected`, `unavailable`, and `no-developer`. These fixtures are not production build entry points.

The optional browser runner uses an externally installed Playwright and Chrome:

```bash
OPPA_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node apps/oppa/test/desktop.browser.mjs
```

The onboarding acceptance runner remains `apps/oppa/test/onboarding.browser.mjs`.
