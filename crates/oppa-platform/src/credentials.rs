use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use tokio::sync::RwLock;
use zeroize::Zeroizing;

use crate::{PlatformError, PlatformResult};

/// Secret UTF-8 credential value that zeroes its allocation on drop.
///
/// `Debug` intentionally never exposes the underlying value.
#[derive(Clone)]
pub struct SecretValue(Zeroizing<String>);

impl SecretValue {
    /// Wraps serialized secret credential material.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(Zeroizing::new(value.into()))
    }

    /// Exposes the value only at an integration boundary that requires it.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretValue([REDACTED])")
    }
}

/// Secure credential storage used by local key and secret-owning layers.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Stores or replaces a secret for a logical account.
    async fn set(&self, account: &str, secret: SecretValue) -> PlatformResult<()>;

    /// Retrieves a secret, returning `Ok(None)` when it does not exist.
    async fn get(&self, account: &str) -> PlatformResult<Option<SecretValue>>;

    /// Removes a secret. Removing a missing secret is idempotent.
    async fn delete(&self, account: &str) -> PlatformResult<()>;
}

/// Native secure-store implementation backed by macOS Keychain, Windows
/// Credential Manager, or freedesktop Secret Service.
#[derive(Debug, Clone)]
pub struct KeyringCredentialStore {
    service: Arc<str>,
}

impl KeyringCredentialStore {
    /// Creates a product-scoped store.
    pub fn new(service: impl Into<String>) -> PlatformResult<Self> {
        let service = service.into();
        validate_key("credential service", &service)?;
        Ok(Self {
            service: Arc::from(service),
        })
    }

    fn entry(service: &str, account: &str) -> PlatformResult<keyring::Entry> {
        validate_key("credential account", account)?;
        keyring::Entry::new(service, account)
            .map_err(|error| PlatformError::CredentialStore(error.to_string()))
    }
}

#[async_trait]
impl CredentialStore for KeyringCredentialStore {
    async fn set(&self, account: &str, secret: SecretValue) -> PlatformResult<()> {
        validate_key("credential account", account)?;
        let service = Arc::clone(&self.service);
        let account = account.to_owned();
        tokio::task::spawn_blocking(move || {
            Self::entry(&service, &account)?
                .set_password(secret.expose_secret())
                .map_err(|error| PlatformError::CredentialStore(error.to_string()))
        })
        .await
        .map_err(|error| PlatformError::Task(error.to_string()))?
    }

    async fn get(&self, account: &str) -> PlatformResult<Option<SecretValue>> {
        validate_key("credential account", account)?;
        let service = Arc::clone(&self.service);
        let account = account.to_owned();
        tokio::task::spawn_blocking(move || {
            let entry = Self::entry(&service, &account)?;
            match entry.get_password() {
                Ok(secret) => Ok(Some(SecretValue::new(secret))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(error) => Err(PlatformError::CredentialStore(error.to_string())),
            }
        })
        .await
        .map_err(|error| PlatformError::Task(error.to_string()))?
    }

    async fn delete(&self, account: &str) -> PlatformResult<()> {
        validate_key("credential account", account)?;
        let service = Arc::clone(&self.service);
        let account = account.to_owned();
        tokio::task::spawn_blocking(move || {
            let entry = Self::entry(&service, &account)?;
            match entry.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(error) => Err(PlatformError::CredentialStore(error.to_string())),
            }
        })
        .await
        .map_err(|error| PlatformError::Task(error.to_string()))?
    }
}

fn validate_key(field: &str, value: &str) -> PlatformResult<()> {
    if value.trim().is_empty() || value.trim() != value || value.len() > 256 {
        return Err(PlatformError::InvalidInput(format!(
            "{field} must be 1 to 256 bytes without surrounding whitespace"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(PlatformError::InvalidInput(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

/// In-memory credential store for deterministic tests and development tools.
///
/// This type provides the same lifecycle semantics but is **not secure** and
/// must never be selected for production credentials.
#[derive(Debug, Clone, Default)]
pub struct MemoryCredentialStore {
    values: Arc<RwLock<HashMap<String, SecretValue>>>,
}

#[async_trait]
impl CredentialStore for MemoryCredentialStore {
    async fn set(&self, account: &str, secret: SecretValue) -> PlatformResult<()> {
        validate_key("credential account", account)?;
        self.values.write().await.insert(account.to_owned(), secret);
        Ok(())
    }

    async fn get(&self, account: &str) -> PlatformResult<Option<SecretValue>> {
        validate_key("credential account", account)?;
        Ok(self.values.read().await.get(account).cloned())
    }

    async fn delete(&self, account: &str) -> PlatformResult<()> {
        validate_key("credential account", account)?;
        self.values.write().await.remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_store_matches_secure_store_lifecycle() {
        let store = MemoryCredentialStore::default();
        assert!(store.get("agent-token").await.expect("read").is_none());
        store
            .set("agent-token", SecretValue::new("secret"))
            .await
            .expect("store");
        assert_eq!(
            store
                .get("agent-token")
                .await
                .expect("read")
                .expect("present")
                .expose_secret(),
            "secret"
        );
        store.delete("agent-token").await.expect("delete");
        store
            .delete("agent-token")
            .await
            .expect("idempotent delete");
    }

    #[test]
    fn secret_debug_output_is_redacted() {
        assert_eq!(
            format!("{:?}", SecretValue::new("do-not-log")),
            "SecretValue([REDACTED])"
        );
    }
}
