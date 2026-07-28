# oppa-product

`oppa-product` makes branded OPPA distributions reproducible and immutable.
At compile time it validates a versioned `product.json`, checks that requested
capabilities are present as Cargo features, and embeds the configuration and
product assets into the binary.

## Build interface

Set one build-time directory:

```bash
OPPA_PRODUCT_DIR=../../products/default cargo build -p oppa-product
```

The directory must contain `product.json` and may contain an `assets/` tree.
When the variable is absent, the repository's `products/default` directory is
used. HTTP and WS endpoints are accepted only for loopback development;
provider endpoints otherwise require TLS.

## Primary APIs

- `embedded_product()` returns strongly typed, validated configuration.
- `embedded_assets()` and `embedded_asset()` expose immutable branded assets.
- `load_product_file()` supports validation tooling and tests; production code
  should not use it as runtime configuration.

This crate depends only on `oppa-core` and serialization/URL libraries. Desktop
and agent layers consume it; it does not know about Tauri or provider business
logic. Product JSON can disable compiled capabilities but cannot enable code
omitted through Cargo features.

## Development

```bash
cargo test -p oppa-product
cargo clippy -p oppa-product --all-targets -- -D warnings
```

The schema is currently version 1. Adding or changing fields requires an
explicit schema-version and migration decision.
