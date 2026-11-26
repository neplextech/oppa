# OPPA store preview artwork

Twelve PNG exports built from dark-mode OPPA application captures:

- `play-store/`: six portrait images, **1080 × 1920**.
- `app-store/`: five layout drafts, **1290 × 2796**.
- `feature-graphic.png`: Android feature graphic, **1024 × 500**.
- `index.html`: a responsive gallery with links to every full-size PNG.
- `artwork.html`: editable HTML/CSS artwork for all variants.
- `manifest.json`: export paths, dimensions, and draft status.

The six subjects are local printing, device pairing, network printer
configuration, job history, virtual receipt previews, and Android
background operation. App Store drafts exclude the Android-only background
operation panel.

## Screenshot provenance

The application images are genuine Android emulator captures, using a
local OpenPrinter example server and test print records. Source PNGs are
in `public/images/setup/mobile-*-dark.png`. The frame hides the operating
system bars with CSS; source screenshots remain intact. No pairing codes
or private credentials appear in the captures.

**The App Store images are design drafts, not final iOS screenshots.**
Replace their Android captures with equivalent native iOS captures before
submission. Dimensions alone do not establish compliance with store review
requirements. The Android screenshot set also needs a final review against
the release being submitted.

Size references:
[Google Play preview assets](https://support.google.com/googleplay/android-developer/answer/9866151),
[Apple screenshot specifications](https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications).

## Regenerate

1. Start the documentation server with `pnpm --filter @oppa/www dev`.
2. Use an external Playwright installation; it is not a runtime dependency
   of the website. For example:

   ```bash
   npm install --prefix /tmp/oppa-preview-render playwright
   OPPA_PLAYWRIGHT_MODULE=/tmp/oppa-preview-render/node_modules/playwright/index.mjs \
     node apps/www/scripts/render-previews.mjs
   ```

The renderer uses installed Google Chrome in headless mode. Set
`OPPA_PREVIEW_BASE_URL` if the site uses another address or port. It waits
for fonts and screenshot images, then exports opaque PNGs at exact pixel
dimensions. The locally hosted Inter font is distributed with its license
in `LICENSE.font.txt`.

For manual edits, open `/previews/artwork.html?format=play-store&slide=0`.
Formats are `play-store`, `app-store`, and `feature`; slide indexes are
zero-based. Set the viewport to the matching export dimensions.
