# OpenPrinter Node.js example

This example is a small host application for local OPPA development. It uses plain Node HTTP,
owns a `ws` upgrade listener and local session registry, feeds each authenticated connection into
`@openprinter/server`, issues short-lived in-memory credentials through a PKCE flow, exposes
connected agents and printer inventories, and delivers test jobs.

The authorization server is intentionally development-only. It binds to `127.0.0.1`, stores
everything in memory, and loses every code and token on restart. Do not expose it to a network or
reuse it in production.

## Run

From the repository root:

```bash
pnpm install
pnpm --filter @openprinter/protocol build
pnpm --filter @openprinter/server build
pnpm --filter openprinter-node-example dev
```

The server listens at `http://127.0.0.1:8787` by default. Set `PORT` to use another port, then
update `product/product.json` to match.

## Point OPPA at the example

The checked-in [`product/product.json`](./product/product.json) points OPPA at:

- `GET /authorize` for browser authorization
- `POST /token` for PKCE code exchange
- `/openprinter/agent` for the WebSocket gateway

Current OPPA builds can set these three endpoints in the app's server settings
without recompiling:

```text
Authorization URL: http://127.0.0.1:8787/authorize
Token URL:         http://127.0.0.1:8787/token
Gateway URL:       ws://127.0.0.1:8787/openprinter/agent
```

The checked-in [`product/product.json`](./product/product.json) remains useful
for branded or automated development builds. The server reads its registered
OAuth client ID from that file so the development provider and agent do not
drift. The example uses insecure `http` and `ws` loopback URLs only for local
development. Production deployments must use `https` and `wss`.

## Development authorization flow

OPPA opens `/authorize` with an OAuth-style authorization-code request:

```text
response_type=code
client_id=oppa-desktop
redirect_uri=http://127.0.0.1:<ephemeral-port>/callback
state=<random>
code_challenge=<base64url-sha256>
code_challenge_method=S256
```

After approval, the example redirects to the loopback callback with a one-time code and the original
state. OPPA posts the code, redirect URI, client ID, and PKCE verifier to `/token`. Codes expire
after two minutes and tokens after one hour. Restart the example to revoke all credentials. The
token response assigns an `agent_id`; OPPA persists that identity with the credential and uses it in
its protocol hello. The gateway rejects a token whose assigned identity differs from the hello. The
example does not issue refresh tokens; authorize again after a token expires.

## HTTP API

### `GET /agents`

Returns snapshots of authenticated, connected agents. Tokens are never included.

### `GET /agents/:agentId/printers`

Returns the most recent validated printer inventory for an online agent.
The included dashboard renders each descriptor's human-readable `name` as the
primary label and keeps its stable printer ID as secondary detail.

### `POST /agents/:agentId/jobs`

Delivers one protocol `PrintJob` JSON object to an online agent.

```bash
curl --fail-with-body \
  -H 'content-type: application/json' \
  --data '{
    "jobId": "job_demo_1",
    "idempotencyKey": "demo_1",
    "printerId": "virtual_printer",
    "createdAt": "2026-07-28T10:00:00.000Z",
    "document": {
      "width": 80,
      "sections": [
        {
          "type": "text",
          "value": "Hello from OpenPrinter",
          "align": "center",
          "bold": true
        },
        { "type": "cut" }
      ]
    }
  }' \
  http://127.0.0.1:8787/agents/agent_demo/jobs
```

A `503` response with `reason: "agent-offline"` is a prompt for the host application to retain or
queue the job durably. This example does not do that for you.

## Event logging

The process logs connection, inventory, acknowledgement, submission, failure, authentication,
heartbeat, and protocol events. It logs identifiers and counts only—not tokens or complete print
documents.

## Transport ownership

This example intentionally keeps framework and transport responsibilities
visible. It parses the Bearer header, authenticates the token, accepts the
WebSocket upgrade, maps `ws` send/close behavior to the SDK transport, and
stores negotiated sessions in a process-local map. A clustered production host
would replace that local routing policy with its own affinity and backplane;
the SDK itself does not coordinate workers or include a broker.

## Development checks

```bash
pnpm --filter openprinter-node-example check
pnpm --filter openprinter-node-example build
```
