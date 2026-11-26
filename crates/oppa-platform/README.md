# oppa-platform

`oppa-platform` is the portable boundary between the reusable agent and
operating-system services.

## Implemented capabilities

- Native secure credential storage through macOS Keychain, Windows Credential
  Manager, or freedesktop Secret Service (`KeyringCredentialStore`, desktop
  only, because `keyring` would silently fall back to a non-persistent mock
  store on Android)
- iOS Keychain storage with `AfterFirstUnlockThisDeviceOnly` accessibility and
  no iCloud synchronization (`AppleKeychainCredentialStore`)
- Host-supplied sandbox directories for mobile (`AppPaths::from_directories`)
- A zeroizing, redacted secret value type
- Product-scoped application data/cache/log paths
- Product documentation/support/legal link opening restricted to credential-free HTTP(S) URLs
- Sanitized platform and hostname metadata

The crate also defines narrow start-on-login, notification, power, and
single-instance interfaces. Until a desktop host provides an implementation,
the supplied fallback types return explicit `Unsupported` errors; they never
pretend an operation succeeded.

## Primary APIs

- `CredentialStore`, `KeyringCredentialStore`, `AppleKeychainCredentialStore`,
  and `SecretValue`. Android Keystore storage is implemented by the Tauri
  host's mobile plugin, because the Keystore is only reachable through the
  Android runtime.
- `MemoryCredentialStore` for tests only
- `resolve_app_paths()` (desktop conventions) and `AppPaths::from_directories()`
- `BrowserOpener`, `SystemBrowser` (desktop), and `validate_browser_url()`
- `StartupManager`, `NotificationService`, `PowerEventSource`,
  `SingleInstanceManager`, and explicit unsupported fallbacks

This crate has no Tauri dependency. `oppa-auth` consumes its secure credential
boundary, while the desktop host uses browser and other native integrations.

## Development

```bash
cargo test -p oppa-platform
cargo clippy -p oppa-platform --all-targets -- -D warnings
```

Keyring integration tests intentionally use the memory store and do not modify
a developer's real credential vault.
