# Protocol package guidance

`@openprinter/protocol` is the canonical source for the OpenPrinter
wire contract. Change TypeBox schemas first, regenerate
`../../protocol/schema/openprinter.schema.json`, mirror the change in
`../../crates/oppa-protocol`, and update shared fixtures in the same
change.

Keep message discriminators and serialized field names stable. All
objects are closed to unknown properties, all collections and strings
must remain bounded, and codecs must reject unsupported versions
before routing. Never add secrets, local filesystem paths, commands,
scripts, or generic proxy requests to the protocol.

Protocol version `"1"` is a string. Discovery, pairing, and gateway
authentication schemas are part of the canonical contract. The
protocol may model a public Ed25519 JWK and opaque challenge frames,
but private-key generation, storage, and signing belong outside this
package.

Use `received`, `submitted`, and `failed` precisely. `submitted` is
backend acceptance, not proof of physical output. Preserve
at-least-once delivery using both `jobId` and `idempotencyKey`; a
manual reprint uses a new job ID.

Public schemas, types, functions, and errors require useful JSDoc.
Run:

```sh
pnpm build
pnpm test
pnpm schema:check
```
