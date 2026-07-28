# Default product assets

The open-source OPPA build falls back to the application icons under
`apps/oppa/src-tauri/icons`. A branded build can place replacements here using
the same filenames as the Tauri icon set (`icon.png`, `icon.ico`, `icon.icns`,
`32x32.png`, `128x128.png`, and the platform-specific square variants). The
product-aware Tauri wrapper selects any matching replacements.

All files in this directory are also compiled into the Rust product crate. They
are never loaded from an editable runtime configuration directory.
