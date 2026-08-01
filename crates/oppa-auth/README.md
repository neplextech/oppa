# `oppa-auth`

Agent-side OpenPrinter pairing and Ed25519 credential management.

## Responsibilities

- validate and normalize one configured OpenPrinter server base URL
- fetch and validate `/.well-known/openprinter`
- resolve relative pairing and gateway endpoints safely
- generate an Ed25519 key pair locally
- keep the private key behind `oppa-platform::CredentialStore`
- expose only the public JWK, credential reference, and challenge-signing operations
- submit pairing requests and validate bounded responses

The crate never returns private key bytes to callers and never writes them to SQLite or ordinary settings. It does not own the WebSocket lifecycle, application UI, server public-key storage, or application authorization policy.

Production server URLs require TLS. Plain `http:` is accepted only for loopback development. Embedded URL credentials and fragments are rejected.

## Public API

- `AgentKeyManager` generates, checks, signs with, and deletes installation-scoped credentials.
- `PairingClient` discovers a server and redeems a pairing code with the generated public key.
- `normalize_server_url` enforces the supported URL and security rules.

## Development

```bash
cargo test -p oppa-auth
cargo clippy -p oppa-auth --all-targets --all-features -- -D warnings
```
