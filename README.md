# Retirement Calculator

This repository contains the Rust workspace foundation for the local-only
retirement calculator. The executable thin slice serves a Leptos CSR build from
Axum and exposes `GET /api/v1/health/live`.

The server binds to `127.0.0.1:3000` by default. Set
`RETIREMENT_CALCULATOR_BIND` only for local development; public exposure is not
supported. CORS is intentionally disabled.

## Toolchain

- Rust `1.88.0`, including `rustfmt`, `clippy`, and `wasm32-unknown-unknown`
- `wasm-bindgen-cli` `0.2.100`
- Node `22.16.0` in CI
- Playwright `1.63.0` with Chromium for the browser smoke test

The issue workspace has a self-contained toolchain under `.toolchain`. Activate
it in a new shell before running Rust commands:

```sh
. scripts/activate-rust-toolchain.sh
```

## Build and verify

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --manifest-path crates/domain/Cargo.toml
bash scripts/check-dependencies.sh
bash scripts/build-web.sh
npm ci --include=dev
cargo run --locked -p server
```

With the server running in another terminal, run `npm run smoke`. The browser
test loads the versioned WASM client through Axum and verifies that the Leptos
UI decoded the frozen v1 health fixture.
