# oppa-platform

`oppa-platform` is the portable boundary between the reusable agent and
operating-system services.

## Implemented capabilities

- Native secure credential storage through macOS Keychain, Windows Credential
  Manager, or freedesktop Secret Service
- A zeroizing, redacted secret value type
- Product-scoped application data/cache/log paths
- System browser opening restricted to credential-free HTTP(S) URLs
- Sanitized platform and hostname metadata

The crate also defines narrow start-on-login, notification, power, and
single-instance interfaces. Until a desktop host provides an implementation,
the supplied fallback types return explicit `Unsupported` errors; they never
pretend an operation succeeded.

## Primary APIs

- `CredentialStore`, `KeyringCredentialStore`, and `SecretValue`
- `MemoryCredentialStore` for tests only
- `resolve_app_paths()`
- `BrowserOpener` and `SystemBrowser`
- `StartupManager`, `NotificationService`, `PowerEventSource`,
  `SingleInstanceManager`, and explicit unsupported fallbacks

This crate has no Tauri dependency. `oppa-auth` consumes its secure credential
and browser boundaries, while the desktop host may adapt native integrations.

## Development

```bash
cargo test -p oppa-platform
cargo clippy -p oppa-platform --all-targets -- -D warnings
```

Keyring integration tests intentionally use the memory store and do not modify
a developer's real credential vault.
