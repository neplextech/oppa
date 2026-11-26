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

impl AppPaths {
    /// Builds and creates product-scoped directories supplied by a host.
    ///
    /// Mobile hosts call this with sandbox directories reported by the
    /// application runtime (for example Android `Context.getFilesDir()` or the
    /// iOS Application Support container), because the desktop home-directory
    /// conventions used by [`resolve_app_paths`] do not exist there. Every
    /// path must be absolute.
    pub fn from_directories(
        data_dir: PathBuf,
        cache_dir: PathBuf,
        log_dir: PathBuf,
    ) -> PlatformResult<Self> {
        let paths = Self {
            data_dir,
            cache_dir,
            log_dir,
        };
        paths.create()?;
        Ok(paths)
    }

    fn create(&self) -> PlatformResult<()> {
        for path in [&self.data_dir, &self.cache_dir, &self.log_dir] {
            if !path.is_absolute() {
                return Err(PlatformError::DataPath(format!(
                    "{} is not an absolute path",
                    path.display()
                )));
            }
            fs::create_dir_all(path).map_err(|error| {
                PlatformError::DataPath(format!("cannot create {}: {error}", path.display()))
            })?;
        }
        Ok(())
    }
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
    paths.create()?;
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::AppPaths;

    #[test]
    fn host_supplied_directories_are_created() {
        let root = std::env::temp_dir().join(format!("oppa-paths-{}", std::process::id()));
        let paths =
            AppPaths::from_directories(root.join("data"), root.join("cache"), root.join("logs"))
                .expect("directories");
        assert!(paths.data_dir.is_dir());
        assert!(paths.cache_dir.is_dir());
        assert!(paths.log_dir.is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn relative_host_directories_are_rejected() {
        assert!(AppPaths::from_directories("data".into(), "cache".into(), "logs".into()).is_err());
    }
}
