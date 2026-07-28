# `@openprinter/protocol`

Canonical TypeBox schemas, inferred TypeScript types, and cross-runtime JSON codecs for the
OpenPrinter wire protocol.

## Responsibilities

- define the versioned agent/server wire contract
- validate every inbound and outbound message at runtime
- model structured print documents and concrete printer descriptors
- enforce bounded messages, documents, metadata, and image data
- generate the committed language-neutral JSON Schema

Rendering, transport authentication, durable queues, printer discovery, and physical submission
belong to other packages and crates.

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

`agent.job_received` means the job is durably stored. `agent.job_submitted` means a backend accepted
it. Neither state universally proves that paper was physically printed.

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

Protocol version 1 includes every initial agent/server message, all structured document primitives,
printer descriptors, print jobs, runtime codecs, and cross-language fixtures. It is the dependency
boundary consumed by the server SDK and Rust agent crates. Other protocol versions are rejected
explicitly until their schemas and compatibility fixtures are implemented.
