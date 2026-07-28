# Node example contributor guide

This example demonstrates how a host application integrates
`@openprinter/server` without a framework, database, or queue service.

## Local constraints

- Use plain Node HTTP APIs so SDK/runtime ownership stays visible.
- Keep authorization codes, tokens, and application state in memory.
- The OAuth-style flow is development-only, requires PKCE S256,
  accepts only loopback redirect URIs, expires codes promptly, and
  consumes each code once.
- Never log access tokens, authorization codes, PKCE verifiers, or
  print document contents.
- Keep endpoint errors structured and cap request-body sizes.
- Retain no hidden durable jobs. The POST job endpoint demonstrates
  immediate delivery and returns a retryable offline result to the
  caller.
- Use generic printer terminology. Product-specific routing belongs
  outside OpenPrinter.
- Keep `product/product.json` aligned with the server's documented
  host, port, authorization, token, and gateway paths.

## Development

```bash
pnpm --filter openprinter-node-example check
pnpm --filter openprinter-node-example dev
```

The server binds to loopback by default. Do not weaken this default
while the development token issuer is enabled.
