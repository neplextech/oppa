# `oppa-documents`

Authenticated, bounded, and integrity-checked retrieval of PDF documents referenced by print jobs.

## Responsibilities

- define the `DocumentFetcher` boundary used by the agent's job processor
- fetch a job's PDF over HTTPS, or over plain HTTP only for loopback development hosts
- sign each request with the installed agent's Ed25519 credential through `oppa-auth`
- refuse redirects, enforce the declared byte length while streaming, and verify the SHA-256 digest before returning bytes
- classify failures as retryable (transport, timeout, server error) or permanent (refused, missing, integrity)

## Non-responsibilities

- rendering, rasterizing, or validating PDF structure (`oppa-renderer`)
- printer submission (`oppa-spooler`)
- durable job state; job payloads never contain secrets, and downloaded bytes are not persisted
- server-side document storage or signature verification (see the request signature section of the crate docs)

## Public API

- `DocumentFetcher`: async trait implemented by `HttpDocumentFetcher` and by test doubles
- `HttpDocumentFetcher::new`: builds a fetcher for one agent identity and credential reference
- `DocumentFetchError`: stable `code()` and `retryable()` classification
- `verify_digest` and `canonical_request`: the integrity and signature primitives, shared with server verifiers

## Dependency role

Depends on `oppa-auth` for signing, and on `oppa-protocol` for the `PdfSource` contract. Low-level crates must not depend on `oppa-agent`, so the agent depends on this crate only through the trait.

## Commands

```bash
cargo test -p oppa-documents
cargo clippy -p oppa-documents --all-targets --all-features -- -D warnings
```

## Status

Implemented for HTTPS and loopback retrieval. Server-side verification is provided by `@openprinter/server` helpers.
