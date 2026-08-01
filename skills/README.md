# OpenPrinter Skills

Agent-friendly documentation for the OpenPrinter SDK packages published from this repository.

Install in your project with:

```bash
npx skills add neplextech/oppa
```

---

## Skills in this collection

### [@openprinter/protocol](./openprinter-protocol/SKILL.md)

The canonical TypeBox schema, runtime codec, and TypeScript types for the OpenPrinter wire protocol v1.

Use this skill when you need to:
- Understand the full message structure for either direction (agent → server, server → agent)
- Parse or validate inbound protocol frames
- Encode outbound messages for a custom transport
- Understand `PrintDocument`, `PrintJob`, `PrinterDescriptor`, and their constraints
- Work with protocol constants and error classes

### [@openprinter/server](./openprinter-server/SKILL.md)

A framework-neutral protocol-session SDK for building OpenPrinter-compatible server endpoints.

Use this skill when you need to:
- Host an OpenPrinter agent connection in any Node.js (or compatible) environment
- Integrate with WebSocket servers, message brokers, or custom transports
- Receive printer inventories, job status updates, and diagnostics from agents
- Deliver print jobs to connected agents
- Implement heartbeat handling, session lifecycle, and protocol error recovery

---

## What this repository is

**OPPA** (Open Printer Proxy Agent) is a local Tauri desktop utility that runs as the agent side of the OpenPrinter protocol. The packages in this repository — `@openprinter/protocol` and `@openprinter/server` — are the generic, product-neutral SDK layer that any OpenPrinter-compatible implementation can use.

Key design principles:
- Protocol semantics live in `@openprinter/protocol` (TypeBox schemas → JSON Schema → Rust fixtures).
- Session management for server-side hosting lives in `@openprinter/server`.
- The OPPA desktop app is one product built on top; it is not part of the SDK.
- `'submitted'` means the OS or printer backend accepted a job. It does **not** mean paper was produced.
- Private keys remain in the agent secure-storage layer; gateway authentication uses only the
  initial challenge frames, never normal protocol message payloads.

---

## Repository layout

```
packages/protocol/   @openprinter/protocol — schemas, codecs, types
packages/server/     @openprinter/server   — session SDK
protocol/            generated JSON Schema and cross-language fixtures
apps/oppa/           OPPA desktop app (Tauri + React)
crates/              Rust agent runtime
examples/            Node.js integration example
```
