---
name: openprinter-protocol
description: >
  @openprinter/protocol schemas, codecs, discovery, pairing, Ed25519
  gateway authentication, messages, print documents, fixtures, and
  protocol version rules.
---

# OpenPrinter protocol skill

Use this skill when changing or integrating `@openprinter/protocol`,
Rust protocol mirrors, generated JSON Schema, or cross-language
fixtures.

## Ownership

`packages/protocol/src/schemas` is canonical. TypeScript types are
inferred from TypeBox schemas. `pnpm protocol:generate`
deterministically writes `protocol/schema/openprinter.schema.json`;
TypeScript and Rust validate the same committed fixtures.

The protocol package owns data contracts and bounded runtime codecs.
It does not store private keys, run HTTP/WebSocket servers, persist
jobs, render documents, discover local printers, or submit output.

## Version and endpoints

- `PROTOCOL_VERSION` is the string `"1"`, not a number.
- Default discovery: `/.well-known/openprinter`
- Default pairing: `/openprinter/pair`
- Default gateway: `/.well-known/openprinter/gateway`
- Authentication method: `pairing-code-ed25519`
- Signature algorithm: `Ed25519`

Keep the OpenPrinter protocol version separate from the server
implementation version. Unsupported versions must fail explicitly.

## Discovery and pairing

Validate discovery with `parseDiscoveryDocument`. A discovery document
contains stable server identity, display name/version, pairing and
gateway endpoints, the authentication method, and challenge lifetime.

Endpoint values may be relative or absolute. Consumers resolve
relative values against the configured server base URL. Agent code
converts HTTP gateway URLs to WS and HTTPS gateway URLs to WSS while
preserving explicit WS/WSS URLs.

Validate pairing bodies with `parsePairingRequest`. The request
contains:

- protocol version
- a body-carried one-time pairing code
- bounded agent name/version/platform/installation identity
- algorithm `Ed25519`
- a public OKP JWK with `kty: "OKP"`, `crv: "Ed25519"`, and a
  canonical 32-byte `x`

The response contains only `agentId`, `keyId`, `serverId`, and
`pairedAt`. Pairing errors use bounded machine-readable envelopes.

## Gateway authentication

Authentication frames occur before the normal protocol envelope:

1. Server sends `auth.challenge` with an opaque base64url payload and
   expiry.
2. Agent signs the exact decoded payload bytes with its local Ed25519
   private key.
3. Agent sends `auth.response` containing challenge, agent, and key
   identifiers plus the signature.
4. Server returns `auth.accepted` or a bounded `auth.rejected`, then
   closes on rejection.
5. Only after acceptance does `agent.hello` begin normal protocol
   negotiation.

Challenges must be unpredictable, socket-bound, single-use, and
short-lived. Normal OpenPrinter messages are not individually signed.
Use `decodeGatewayAuthenticationResponse`,
`decodeGatewayAuthenticationServerMessage`, and their matching
encoders. Use canonical unpadded RFC 4648 base64url helpers.

## Normal message lifecycle

The server sends `server.hello` after a valid `agent.hello`.
Heartbeats are correlated. Printer inventory is revisioned. Job
semantics are deliberately precise:

- `agent.job_received`: job is durably stored by the agent
- `agent.job_submitted`: a local backend accepted it
- `agent.job_failed`: a bounded structured failure occurred

Do not claim universal physical `printed` completion.

All messages use stable discriminators, `protocolVersion`,
`messageId`, `sentAt`, a payload, and correlation where required.
Decode inbound values before use and encode outbound values through
the codec.

## Change checklist

For every protocol change:

1. Update TypeBox schemas and inferred types.
2. Update codecs, limits, and structured errors.
3. Regenerate the committed JSON Schema.
4. Update valid and invalid fixtures, including Rust mirrors.
5. Add TypeScript and Rust compatibility tests.
6. Document delivery, authentication, and error semantics.

## Commands

```bash
pnpm --filter @openprinter/protocol build
pnpm --filter @openprinter/protocol test
pnpm protocol:generate
cargo test -p oppa-protocol
```
