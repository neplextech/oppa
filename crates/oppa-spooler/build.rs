//! Declares the `cups` configuration used to gate CUPS-backed code.
//!
//! CUPS (`lp`, `lpstat`, `lpoptions`) is available on desktop Unix hosts.
//! Android and iOS are Unix targets without CUPS or user-visible print
//! queues, so `cfg(unix)` is deliberately not used for this boundary.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(cups)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(
        target_os.as_str(),
        "macos" | "linux" | "freebsd" | "openbsd" | "netbsd" | "dragonfly"
    ) {
        println!("cargo::rustc-cfg=cups");
    }
}
