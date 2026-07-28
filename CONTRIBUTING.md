# Contributing to OpenPrinter

Thank you for improving OPPA and the OpenPrinter integration
libraries. This repository contains a security-sensitive desktop
agent, a shared wire protocol, a server SDK, an example provider, and
documentation. Changes should preserve the boundaries described in
[`PLAN.md`](PLAN.md).

## Before you start

- Search existing issues and pull requests for overlapping work.
- Discuss protocol or public API changes before implementation.
- Keep product-specific business rules outside the generic OpenPrinter
  packages.
- Never commit credentials, print payloads captured from real users,
  or unsanitized diagnostics.

## Local setup

Install the toolchains named in `rust-toolchain.toml` and the root
`packageManager` field, then run:

```bash
pnpm install --config.confirmModulesPurge=false
pnpm --filter @openprinter/protocol check
pnpm typecheck
pnpm test
pnpm lint
pnpm format:check
```

The example provider and desktop app can be started in separate
terminals:

```bash
pnpm --filter openprinter-node-example dev
pnpm oppa:dev
```

The default product definition points the desktop agent at the
loopback example provider.

## Change guidelines

- Treat `protocol/schema/openprinter.schema.json` and the shared
  fixtures as cross-language contracts. Regenerate and test both Rust
  and TypeScript consumers after protocol changes.
- Persist an accepted job before reporting `agent.job_received`.
- Use `submitted` only for backend acceptance; do not claim that a
  printer physically printed.
- Keep Tauri commands narrow and typed. Runtime orchestration belongs
  in Rust services and crates.
- Add regression tests at the layer where a defect originated.
- Update the nearest `README.md` and user-facing documentation with
  behavioral changes.

## Pull requests

Keep pull requests focused and explain:

- the behavior being changed;
- security or compatibility implications;
- validation performed;
- runtime or hardware checks that remain for reviewers.

Use Conventional Commit-style titles such as
`feat(agent): recover pending jobs` or
`fix(server): reject mismatched agent identity`. Breaking changes must
be explicit.
