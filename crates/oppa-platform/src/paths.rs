use std::{fs, path::PathBuf};

use directories::ProjectDirs;

use crate::{PlatformError, PlatformResult};

/// Product-scoped filesystem locations for non-secret state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Durable database and local application state.
    pub data_dir: PathBuf,
    /// Bounded caches that may be recreated.
    pub cache_dir: PathBuf,
    /// Sanitized diagnostic logs.
    pub log_dir: PathBuf,
}

/// Resolves and creates product-scoped application directories.
///
/// Secrets must not be written to any returned path; use
/// [`crate::CredentialStore`] instead.
pub fn resolve_app_paths(
    qualifier: &str,
    organization: &str,
    application: &str,
) -> PlatformResult<AppPaths> {
    for (field, value) in [
        ("qualifier", qualifier),
        ("organization", organization),
        ("application", application),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(PlatformError::InvalidInput(format!(
                "{field} must not be empty or contain control characters"
            )));
        }
    }
    let project = ProjectDirs::from(qualifier, organization, application).ok_or_else(|| {
        PlatformError::DataPath("operating system did not provide a home directory".to_owned())
    })?;
    let paths = AppPaths {
        data_dir: project.data_dir().to_path_buf(),
        cache_dir: project.cache_dir().to_path_buf(),
        log_dir: project.data_local_dir().join("logs"),
    };
    for path in [&paths.data_dir, &paths.cache_dir, &paths.log_dir] {
        fs::create_dir_all(path).map_err(|error| {
            PlatformError::DataPath(format!("cannot create {}: {error}", path.display()))
        })?;
    }
    Ok(paths)
}
