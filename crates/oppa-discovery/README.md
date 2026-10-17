# oppa-discovery

`oppa-discovery` runs independent printer providers, normalizes their output,
deduplicates observations with positive fingerprint evidence, and emits
inventory changes.

## Included providers

- Operating-system queues (`lpstat` on Unix and `Get-Printer` on Windows),
  reporting online/offline/degraded availability from the queue state
  (`enabled`/`disabled` on Unix, `PrinterStatus` and `WorkOffline` on Windows)
  and advertising driver-based printing without claiming ESC/POS support
- Manually configured raw TCP printers, with a bounded TCP reachability probe
  that reports online/offline (no bytes are written)
- Mutable in-process virtual printers

Every platform command and provider call has a deadline. An unavailable
provider becomes a diagnostic `ProviderFailure`; it does not crash the agent or
discard healthy results from other providers. USB and mDNS discovery are not
claimed in this initial implementation.

## CUPS queue options

Unix system queues are enriched with `lpoptions -p <queue> -l`. The parser
reads PPD-style keys (`PageSize`, `Duplex`) and IPP-style keys (`media`,
`sides`) and reports only the paper sizes and duplex modes it finds. A failed
or timed-out query reports native PDF submission without claiming media or
duplex support. Windows queues advertise the rasterized PDF route, and
virtual printers advertise PDF only through their configured profiles.

## Primary APIs

- `DiscoveryProvider`
- `DiscoveryManager` and `DiscoverySnapshot`
- `normalize_printer()` and `deduplicate()`
- `SystemQueueProvider`, `ManualNetworkProvider`, and
  `VirtualPrinterProvider`

The crate depends on `oppa-printer` domain types. Stable identity remains a
storage concern: fingerprints are evidence and are not treated as permanent
IDs. Discovery reports a system queue as a driver path; a local explicit raw
language choice belongs to printer configuration and is not inferred from the
queue name.

## Development

```bash
cargo test -p oppa-discovery
cargo clippy -p oppa-discovery --all-targets -- -D warnings
```

Tests parse captured command output and use in-memory providers; CI does not
require a physical printer or an installed queue service.
