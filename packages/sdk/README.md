# openprinter

`openprinter` sdk is a strongly typed TypeScript client for the public Neplex
OpenPrinter Cloud API. It covers project API-key access to service health,
discovery, pairing, and durable print-job submission/listing without exposing
server internals or agent private keys.

Self-hosting OpenPrinter and OPPA is completely free. This package is also
usable against a self-hosted compatible API; Neplex pricing applies only to
the managed Neplex OpenPrinter Cloud API. See the [current managed pricing](https://oppa.neplex.dev/pricing).

## Install

```bash
pnpm add openprinter
```

## Quick start

```ts
import { createOpenPrinterClient } from 'openprinter';

const openprinter = createOpenPrinterClient({
  baseUrl: process.env.OPENPRINTER_BASE_URL,
  apiKey: process.env.OPENPRINTER_API_KEY,
  projectId: process.env.OPENPRINTER_PROJECT_ID,
});

const result = await openprinter.jobs.create({
  printerId: 'printer_01',
  idempotencyKey: 'order_123_receipt',
  document: {
    width: 80,
    sections: [
      { type: 'text', value: 'Thank you', align: 'center', bold: true },
      { type: 'divider' },
      { type: 'row', left: 'Subtotal', right: '$12.00' },
    ],
  },
});

console.log(result.job.id, result.duplicate);
```

`create()` uses the idempotency key when retrying a transient request, so a
network timeout does not require an application-level duplicate guard.

## Configuration

Every option can be supplied explicitly. Omitted values are read from these
Node-compatible environment variables:

| Option             | Environment variable                                                                 | Default                    |
| ------------------ | ------------------------------------------------------------------------------------ | -------------------------- |
| `baseUrl`          | `OPENPRINTER_BASE_URL`, `OPENPRINTER_API_BASE_URL`, or `OPENPRINTER_PUBLIC_BASE_URL` | required                   |
| `apiKey`           | `OPENPRINTER_API_KEY`                                                                | required for jobs          |
| `projectId`        | `OPENPRINTER_PROJECT_ID`                                                             | required for `client.jobs` |
| `timeoutMs`        | `OPENPRINTER_TIMEOUT_MS`                                                             | `30_000`                   |
| `retry.maxRetries` | `OPENPRINTER_MAX_RETRIES`                                                            | `2`                        |

The API key may also be a synchronous or asynchronous provider, which is
useful for key rotation. `fetch`, `headers`, `signal`, timeout, and retry
behavior are configurable for browsers, Node.js, tests, and compatible
runtimes.

```ts
const openprinter = createOpenPrinterClient({
  apiKey: () => keyStore.getCurrentOpenPrinterKey(),
  timeoutMs: 10_000,
  retry: {
    maxRetries: 3,
    baseDelayMs: 200,
    maxDelayMs: 3_000,
  },
});

const project = openprinter.project('project_01');
const jobs = await project.jobs.list({ state: 'FAILED' });
```

The SDK sends `Authorization: Bearer opk_…` for authenticated methods and
never logs or stores the key. Public API-key scopes currently map to
`jobs:read` and `jobs:write`.

## Errors

Catch the exported typed errors when a caller needs to distinguish an API
failure from a malformed response, timeout, or configuration error:

```ts
import { isOpenPrinterApiError, isOpenPrinterTimeoutError } from 'openprinter';

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

All public interfaces, methods, options, and error fields include JSDoc in
the generated declarations. The SDK validates structured documents and
successful JSON responses at runtime using the canonical
`@openprinter/protocol` codecs where applicable.

## API surface

- `client.health()` — unauthenticated service liveness check.
- `client.discovery()` — validate the standard OpenPrinter discovery document.
- `client.pair(request)` — redeem an agent pairing request; private key
  generation and signing remain OPPA responsibilities.
- `client.jobs.create(input)` — submit an idempotent durable print job.
- `client.jobs.list(options)` — list public job projections by state.
- `client.project(projectId)` — create a project-pinned view without mutating
  the client.

The generated API reference and managed-cloud guide are available in the
[OpenPrinter Cloud documentation](https://oppa.neplex.dev/cloud).
