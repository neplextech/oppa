---
name: openprinter
description: >
  The openprinter TypeScript client for the public Neplex OpenPrinter
  Cloud API, including configuration, discovery, pairing, durable
  print jobs, and typed error handling.
---

# openprinter SDK skill

Use this skill when integrating the `openprinter` package into a
TypeScript or JavaScript application that talks to the public Neplex
OpenPrinter Cloud API.

## Install and configure

Install the package with:

```bash
npm install openprinter
```

Create one client for the service origin, API key, and default
project. Values may be passed explicitly or read from
`OPENPRINTER_BASE_URL`, `OPENPRINTER_API_BASE_URL`,
`OPENPRINTER_PUBLIC_BASE_URL`, `OPENPRINTER_API_KEY`, and
`OPENPRINTER_PROJECT_ID`.

```ts
import { createOpenPrinterClient } from 'openprinter';

const openprinter = createOpenPrinterClient({
  baseUrl: process.env.OPENPRINTER_BASE_URL,
  apiKey: process.env.OPENPRINTER_API_KEY,
  projectId: process.env.OPENPRINTER_PROJECT_ID,
});
```

Keep API keys in a trusted server or application boundary. The SDK
sends them as `Authorization: Bearer` credentials and does not log or
store them. Do not ship a project API key in a browser bundle or
expose it to untrusted users.

## Public client surface

- `client.health()` checks service liveness without credentials.
- `client.discovery()` reads and validates the standard discovery
  document.
- `client.pair(request)` submits an agent pairing request. OPPA
  remains responsible for private-key generation and signing.
- `client.jobs.create(input)` submits one durable, idempotent print
  job.
- `client.jobs.list(options)` lists public job projections and can
  filter by `PENDING`, `DISPATCHING`, `DELIVERED`, `PRINTING`,
  `COMPLETED`, `FAILED`, `EXPIRED`, or `CANCELLED`.
- `client.project(projectId)` creates a project-pinned view without
  mutating the client.

For jobs, `printerId`, `idempotencyKey`, and a canonical
`PrintDocument` are required. Use a stable idempotency key for
application retries. Optional metadata, expiry, and maximum delivery
attempts are bounded by the service.

```ts
const result = await openprinter.jobs.create({
  printerId: 'printer_01',
  idempotencyKey: 'order_123_receipt',
  document: {
    width: 80,
    sections: [
      {
        type: 'text',
        value: 'Thank you',
        align: 'center',
        bold: true,
      },
      { type: 'divider' },
    ],
  },
});

console.log(result.job.id, result.duplicate);
```

The client retries transient failures by default. `create()` is safe
to retry because the idempotency key is sent with the request.
Configure `timeoutMs` and `retry`, or set `retry: false`, when the
host needs different behavior.

## Errors and ownership

Use the exported type guards to distinguish API failures, invalid
successful responses, timeouts, and configuration errors:

```ts
import {
  isOpenPrinterApiError,
  isOpenPrinterTimeoutError,
} from 'openprinter';

try {
  await openprinter.jobs.list();
} catch (error) {
  if (isOpenPrinterApiError(error) && error.status === 401) {
    // Rotate or reconfigure the project API key.
  } else if (isOpenPrinterTimeoutError(error)) {
    // Apply application-specific availability handling.
  }
}
```

`openprinter` owns typed HTTP requests, response validation, retries,
and the public API-key surface. It does not own durable application
jobs, printer routing, local discovery, private keys, or gateway
authentication. Keep those responsibilities in the host application,
OPPA, or the appropriate protocol and server package.

Do not describe `COMPLETED` as proof that paper was produced.
OpenPrinter protocol delivery semantics distinguish durable receipt
and backend submission from physical output.

## Related packages and references

- `@openprinter/protocol` owns the canonical wire schemas, codecs, and
  `PrintDocument` types.
- `@openprinter/server` owns framework-neutral agent sessions and
  gateway authentication.
- The package source and complete API guide are in
  `packages/sdk/README.md`.
- Managed-cloud documentation: `https://oppa.neplex.dev/cloud`.

Useful package checks from the repository root:

```bash
pnpm --filter openprinter build
pnpm --filter openprinter test
```
