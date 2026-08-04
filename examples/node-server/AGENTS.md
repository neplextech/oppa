# Node example guidance

- Keep the example generic and disposable. It demonstrates OpenPrinter
  integration, not application-specific routing or persistence.
- Use `@openprinter/server` discovery, pairing, public credential
  stores, challenge authentication, and session APIs directly.
- The included stores are volatile and development-only. Production
  guidance must require durable pairing and public credential storage
  plus rate limiting.
- Pairing codes may be logged only by this local example and must
  never be placed in a URL or query parameter.
- Never log private keys, signatures, complete challenges, pairing
  requests, or print documents.
- Expose one configured base URL. Use the SDK's default discovery,
  pairing, and gateway paths unless a test explicitly covers
  overrides.
- Keep WebSocket upgrade ownership in the example host and pass
  accepted sockets through `handleGatewayConnection`.
- Preserve host ownership of live-session routing and durable jobs.
- Tests must use virtual printers and in-memory stores; no physical
  printer or external service is required.

## Dev routes (development only, not for production)

| Method | Path | Purpose |
|--------|------|---------|
| `GET` | `/dev` | Browser UI: pairing deep link generator + agent/printer view |
| `GET` | `/agents` | JSON list of connected agents |
| `GET` | `/agents/:agentId/printers` | Printer inventory for one agent |
| `POST` | `/development/pairing-code` | Create a one-time pairing code |
| `POST` | `/development/test-print/:agentId/:printerId` | Send a test job to a printer |

The deep link shown in the `/dev` UI follows the format documented in
`apps/www/content/docs/deep-link-pairing.mdx` and in the root
`AGENTS.md` deep link pairing section.
