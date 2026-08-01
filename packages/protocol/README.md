# `@openprinter/protocol`

Canonical TypeBox schemas, inferred TypeScript types, and cross-runtime JSON codecs for the
OpenPrinter wire protocol.

## Responsibilities

- define the versioned discovery, pairing, authentication, and agent/server wire contracts
- validate every inbound and outbound message at runtime
- model structured print documents and concrete printer descriptors
- enforce bounded messages, documents, metadata, and image data
- generate the committed language-neutral JSON Schema

The package defines authentication data and validates public keys and frames; private-key storage,
signature execution, durable credentials, rendering, queues, discovery of local printers, and
physical submission belong to other packages and crates.

## Install

```sh
pnpm add @openprinter/protocol
```

## Usage

```ts
import {
  PROTOCOL_VERSION,
  decodeAgentMessage,
  encodeServerMessage,
  type ServerMessage,
} from "@openprinter/protocol";

const inbound = decodeAgentMessage(webSocketPayload);

const heartbeat: ServerMessage = {
  protocolVersion: PROTOCOL_VERSION,
  messageId: "msg_heartbeat_42",
  sentAt: "2026-07-28T10:00:00Z",
  type: "server.heartbeat",
  payload: { timeoutMs: 15_000 },
};

socket.send(encodeServerMessage(heartbeat));
```

`PROTOCOL_VERSION` is the string `"1"`. Discovery is validated with
`parseDiscoveryDocument`, pairing requests with `parsePairingRequest`, and gateway authentication
frames with the corresponding `decodeGatewayAuthentication*` codecs. Ed25519 keys use public OKP
JWKs and canonical unpadded base64url values.

`agent.job_received` means the job is durably stored. `agent.job_submitted` means a backend accepted
it. Neither state universally proves that paper was physically printed.

`server.hello` includes required `OpenPrinterBrandMetadata` with a bounded
human-readable service name. It intentionally supports no icon or external
resource URL.

Printer descriptors omit `capabilities` when discovery cannot determine them. An absent capability
block means unknown; senders must not invent a media width or feature flags merely to populate the
field.

## Source of truth

The TypeBox schemas under `src/schemas` are canonical. The deterministic generator emits
`../../protocol/schema/openprinter.schema.json`; Rust mirrors the schema and validates the same
fixtures under `../../protocol/fixtures`. TypeBox evaluates string `minLength` and `maxLength`
against JavaScript string length, so supplementary Unicode characters count as two UTF-16 code
units; mirrors must preserve that boundary behavior.

```sh
pnpm build
pnpm test
pnpm schema:generate
pnpm schema:check
pnpm check
```

The public runtime API uses only standard `TextEncoder`, `TextDecoder`, and JSON APIs, so it is
usable in modern Node.js, Bun, Deno, and browsers. The schema generator and test harness are
development-only Node.js tools.

## Current status

Protocol version 1 includes server discovery, pairing, challenge authentication, every initial
agent/server message, structured document primitives, printer descriptors, print jobs, runtime
codecs, and cross-language fixtures. Other versions are rejected explicitly.
