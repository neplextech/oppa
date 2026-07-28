# oppa-transport

`oppa-transport` provides the Tauri-independent WebSocket transport between an OPPA agent and an
OpenPrinter-compatible server.

## Behavior

- Requires WSS for remote gateways and permits WS only on loopback development
- Carries the bearer token only in the HTTP `Authorization` header
- Classifies HTTP 401/403 handshakes as non-retryable credential rejection
- Uses canonical `oppa-protocol` codecs for every application message
- Requires an explicit `agent.hello` as the first application message on every socket; buffered
  events flush only after that hello succeeds
- Caps Tungstenite messages and individual frames at the canonical 2 MiB wire limit before protocol
  decoding
- Tracks observable disconnected/connecting/connected/backoff/closing states
- Enforces connect, idle/heartbeat, and graceful-close deadlines
- Handles WebSocket ping/pong and clean close frames
- Reconnects with bounded exponential backoff and configurable jitter
- Supports cooperative cancellation and a small, explicit outbound event buffer

`agent.hello` cannot be placed in the event buffer. After `connect`, the host must call
`send(hello)` before sending or receiving any other application message. This invariant is reset on
every reconnect.

Durable job delivery is not entrusted to the in-memory buffer. Jobs and outcomes remain in
`oppa-storage` and are reconciled by `oppa-agent`.

## Primary APIs

- `AgentTransport` and `WebSocketTransport`
- `TransportConfig`
- `ConnectionState`
- `BackoffPolicy` and `ReconnectBackoff`
- `TransportError`

## Development

```bash
cargo test -p oppa-transport
cargo clippy -p oppa-transport --all-targets -- -D warnings
```

Transport tests use a local WebSocket peer and shared cross-language protocol fixtures; no external
server is required.
