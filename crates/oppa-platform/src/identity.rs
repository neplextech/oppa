use crate::{PlatformError, PlatformResult};

/// Sanitized device metadata suitable for diagnostics and agent hello.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformInfo {
    /// Rust target operating system.
    pub operating_system: &'static str,
    /// Rust target CPU architecture.
    pub architecture: &'static str,
    /// Hostname, when it is valid Unicode.
    pub hostname: Option<String>,
}

/// Reads portable platform metadata without exposing usernames or paths.
pub fn platform_info() -> PlatformResult<PlatformInfo> {
    let hostname = hostname::get()
        .map_err(|error| PlatformError::Task(format!("cannot read hostname: {error}")))?
        .into_string()
        .ok();
    Ok(PlatformInfo {
        operating_system: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        hostname,
    })
}
