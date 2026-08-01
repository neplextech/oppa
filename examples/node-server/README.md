# OpenPrinter Node.js example

A disposable local integration for `@openprinter/protocol`, `@openprinter/server`, and OPPA. It exposes HTTP discovery and pairing, upgrades the authenticated gateway with `ws`, keeps a process-local session registry, and provides a test job endpoint.

The example binds to `127.0.0.1`, uses volatile in-memory pairing and public credential stores, and logs a pairing code for local development. Restarting it removes every code and paired credential. Production applications must use durable stores, authorization around code creation, and pairing-attempt rate limits.

## Run

```bash
pnpm --filter openprinter-node-example dev
```

The default base URL is `http://127.0.0.1:8787`. Set `PORT` to select another loopback port.

At startup the process logs a short-lived code like `ABCD-EFGH`. Configure OPPA with the base URL, let it discover the server, then enter that code and an agent name. OPPA generates the Ed25519 key locally and sends only its public JWK.

## Routes

| Route | Purpose |
| --- | --- |
| `GET /.well-known/openprinter` | Validated server identity, endpoint, and authentication metadata |
| `POST /openprinter/pair` | Redeem one pairing code and register an agent public key |
| `GET /.well-known/openprinter/gateway` (upgrade) | Challenge-authenticated OpenPrinter WebSocket |
| `POST /development/pairing-code` | Create another local-development pairing code |
| `POST /agents/:agentId/jobs` | Send a validated print job to one connected agent |
| `GET /` | List routes and process-local connected agent IDs |

Pairing codes are request-body data, never query parameters. The server sends an unpredictable, expiring, socket-bound challenge. OPPA signs it with its locally stored key; after `auth.accepted`, the normal `agent.hello` and OpenPrinter traffic begin. Ordinary messages are not individually signed.

## Send a virtual job

After OPPA is paired, connected, and advertising a virtual printer:

```bash
curl -X POST http://127.0.0.1:8787/agents/AGENT_ID/jobs \
  -H 'content-type: application/json' \
  -d '{
    "jobId": "job_example_001",
    "idempotencyKey": "job_example_001_v1",
    "printerId": "VIRTUAL_PRINTER_ID",
    "createdAt": "2026-08-01T09:00:00Z",
    "document": {
      "width": 80,
      "sections": [{ "type": "text", "value": "OpenPrinter example" }]
    }
  }'
```

The endpoint reports transport delivery only. `agent.job_received` confirms local durable receipt; `agent.job_submitted` confirms backend acceptance and does not universally prove physical output.

## Validation

```bash
pnpm --filter openprinter-node-example check
```

The integration test covers discovery, pairing, Ed25519 challenge authentication, job acknowledgement, and reconnecting with the same credential.
