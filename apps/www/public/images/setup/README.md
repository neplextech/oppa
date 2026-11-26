# Setup guide screenshots

Dark-mode captures of the actual OPPA React interface using its existing
development fixtures and example data. No installation dialogs or real
pairing credentials are included. Each PNG is 960 × 640.

To refresh, run `pnpm --filter oppa dev`, set the browser viewport to
960 × 640, and capture these application views:

- `connect-dark.png`: `/test/onboarding.html?step=connect&theme=dark`
- `pair-dark.png`: `/test/onboarding.html?step=pair&theme=dark`
- `printers-dark.png`: `/test/onboarding.html?step=printers&theme=dark`
- `overview-dark.png`: `/test/desktop.html?theme=dark&no-developer#/overview`
- `test-print-dark.png`: `/test/desktop.html?theme=dark&no-developer#/printers?selected=printer_system_frontdesk`
- `add-printer-dark.png`: the previous view, then select **Add printer**
- `virtual-dark.png`: `/test/desktop.html?theme=dark#/virtual?selected=printer_virtual_receipts`

Keep screenshot labels and instructions in `content/docs/setup.mdx`
aligned with the current application. The fixtures are excluded from the
production application bundle.

## Android application captures

`mobile-pair-dark.png`, `mobile-add-printer-dark.png`,
`mobile-background-dark.png`, `mobile-virtual-dark.png`, and
`mobile-jobs-dark.png` are 1080 × 2220 captures from the Android Tauri app,
using the development example server and virtual test output. They are
also the source images for the artwork in `public/previews`.

Use Android's dark appearance, keep one-time pairing codes out of saved
images, and capture only the application. The dedicated mobile guide is
`content/docs/mobile-setup.mdx`.
