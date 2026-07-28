# oppa-printer

`oppa-printer` defines the printer domain boundary used throughout OPPA. It
models connections, capabilities, availability, discovery observations,
configured references, and submission acknowledgements without performing
platform I/O.

## Responsibilities

- Keep stable OPPA printer IDs separate from mutable discovery fingerprints.
- Model system queues, raw network endpoints, USB descriptors, and virtual
  printers as validated tagged values.
- Preserve provenance when multiple discovery providers observe one device.
- Define the async discovery/capability backend contract.

The crate does not render documents, submit bytes, persist identities, or make
MAC addresses a universal identity. Rendering belongs to `oppa-renderer`,
submission to `oppa-spooler`, reconciliation to `oppa-discovery`, and durable
configuration to `oppa-storage`.

## Primary APIs

- `PrinterConnection`, `PrinterFingerprint`, and `PrinterRef`
- `DiscoveredPrinter` and `ProviderMetadata`
- `PrinterCapabilities` and `PrinterAvailability`
- `PrinterBackend`
- `SubmissionReceipt`

This is a low-level crate depending only on `oppa-core` and serialization/error
support.

## Development

```bash
cargo test -p oppa-printer
cargo clippy -p oppa-printer --all-targets -- -D warnings
```

The implementation is ready for discovery and spooler integrations. Direct USB
transport is deliberately not claimed.
