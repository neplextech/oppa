//! Registers the native Android library and Swift package with Tauri.
//!
//! The plugin exposes no frontend commands: every native call is made by the
//! OPPA host in Rust, so the command list is intentionally empty.

const COMMANDS: &[&str] = &[];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
