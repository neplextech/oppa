# oppa-auth

`oppa-auth` implements OPPA's provider-neutral public-client authorization
flow. It creates RFC 7636 PKCE values, validates high-entropy state, listens on
an ephemeral IPv4 loopback port, exchanges and refreshes tokens, and persists
credentials only through `oppa-platform::CredentialStore`.

## Security invariants

- The callback binds only to `127.0.0.1`, expires within a bounded lifetime,
  accepts one valid authorization code, validates state, and then closes.
- Authorization and token endpoints require TLS except for explicit loopback
  development.
- Token HTTP redirects are disabled and response size is bounded.
- The initial token response must carry a provider-issued `agent_id`; refresh
  responses may omit it and preserve the securely stored identity. Agent hello
  uses this exact ID so transport authentication and protocol identity agree.
- Access tokens, refresh tokens, PKCE verifiers, authorization codes, and state
  are redacted from `Debug` output.
- Credential serialization is handed directly to native secure storage, never
  SQLite or JSON files.

The integrating provider remains responsible for login, account/organization
selection, approval, permissions, and token policy. This crate does not define
authentication plugins or a token format.

## Primary APIs

- `AuthorizationClient` and `PendingAuthorization`
- `PkcePair` and `AuthorizationState`
- `LoopbackCallback`
- `TokenSet` and `CredentialManager`
- `AuthStateTracker`

## Development

```bash
cargo test -p oppa-auth
cargo clippy -p oppa-auth --all-targets -- -D warnings
```

Tests use an in-memory credential boundary and real loopback sockets; they do
not open a browser or contact a provider.
