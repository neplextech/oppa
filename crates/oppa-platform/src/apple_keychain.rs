//! iOS Keychain credential storage with device-bound accessibility.
//!
//! The desktop [`crate::KeyringCredentialStore`] uses the `keyring` crate,
//! whose iOS backend cannot choose a Keychain accessibility class. Mobile
//! agents need a stricter policy: secrets are readable after the first unlock
//! (so a bounded background task can finish a reconnect) and are never
//! migrated to another device through backups or iCloud Keychain.

use std::sync::Arc;

use async_trait::async_trait;
use security_framework::{
    access_control::{ProtectionMode, SecAccessControl},
    passwords::{
        PasswordOptions, delete_generic_password_options, generic_password,
        set_generic_password_options,
    },
};
use security_framework_sys::base::errSecItemNotFound;

use crate::{
    CredentialStore, PlatformError, PlatformResult, SecretValue, credentials::validate_key,
};

/// Generic-password Keychain store scoped to one product service name.
///
/// Items use `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` and are not
/// synchronizable, so they never leave the device that generated them.
#[derive(Debug, Clone)]
pub struct AppleKeychainCredentialStore {
    service: Arc<str>,
}

impl AppleKeychainCredentialStore {
    /// Creates a product-scoped Keychain store.
    pub fn new(service: impl Into<String>) -> PlatformResult<Self> {
        let service = service.into();
        validate_key("credential service", &service)?;
        Ok(Self {
            service: Arc::from(service),
        })
    }

    fn lookup(service: &str, account: &str) -> PasswordOptions {
        let mut options = PasswordOptions::new_generic_password(service, account);
        options.set_access_synchronized(Some(false));
        options
    }
}

fn keychain_error(error: &security_framework::base::Error) -> PlatformError {
    PlatformError::CredentialStore(format!("keychain status {}", error.code()))
}

#[async_trait]
impl CredentialStore for AppleKeychainCredentialStore {
    async fn set(&self, account: &str, secret: SecretValue) -> PlatformResult<()> {
        validate_key("credential account", account)?;
        let service = Arc::clone(&self.service);
        let account = account.to_owned();
        tokio::task::spawn_blocking(move || {
            // Replace rather than update so the accessibility class of an
            // older item can never survive a credential rotation.
            match delete_generic_password_options(Self::lookup(&service, &account)) {
                Ok(()) => {}
                Err(error) if error.code() == errSecItemNotFound => {}
                Err(error) => return Err(keychain_error(&error)),
            }
            let access_control = SecAccessControl::create_with_protection(
                Some(ProtectionMode::AccessibleAfterFirstUnlockThisDeviceOnly),
                0,
            )
            .map_err(|error| keychain_error(&error))?;
            let mut options = Self::lookup(&service, &account);
            options.set_access_control(access_control);
            set_generic_password_options(secret.expose_secret().as_bytes(), options)
                .map_err(|error| keychain_error(&error))
        })
        .await
        .map_err(|error| PlatformError::Task(error.to_string()))?
    }

    async fn get(&self, account: &str) -> PlatformResult<Option<SecretValue>> {
        validate_key("credential account", account)?;
        let service = Arc::clone(&self.service);
        let account = account.to_owned();
        tokio::task::spawn_blocking(move || {
            match generic_password(Self::lookup(&service, &account)) {
                Ok(bytes) => {
                    let bytes = zeroize::Zeroizing::new(bytes);
                    let value = std::str::from_utf8(&bytes).map_err(|_| {
                        PlatformError::CredentialStore(
                            "keychain item is not valid UTF-8".to_owned(),
                        )
                    })?;
                    Ok(Some(SecretValue::new(value)))
                }
                Err(error) if error.code() == errSecItemNotFound => Ok(None),
                Err(error) => Err(keychain_error(&error)),
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
            match delete_generic_password_options(Self::lookup(&service, &account)) {
                Ok(()) => Ok(()),
                Err(error) if error.code() == errSecItemNotFound => Ok(()),
                Err(error) => Err(keychain_error(&error)),
            }
        })
        .await
        .map_err(|error| PlatformError::Task(error.to_string()))?
    }
}
