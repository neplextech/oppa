# `oppa-transport`

Bounded OpenPrinter WebSocket transport for the Rust agent.

## Responsibilities

- connect to a discovered `ws:` or `wss:` gateway URL
- receive the initial authentication challenge
- send the agent's Ed25519 response and require `auth.accepted`
- encode, decode, and size-limit normal protocol messages after authentication
- apply connection, read, write, close, and heartbeat timeouts
- expose structured transport and authentication errors

The transport never stores credentials, generates keys, discovers server endpoints, renders documents, submits printer jobs, or signs ordinary protocol messages. The caller supplies the challenge response produced through `oppa-auth`; TLS protects subsequent session traffic.

## Development

```bash
cargo test -p oppa-transport
cargo clippy -p oppa-transport --all-targets --all-features -- -D warnings
```
