# Retirement Calculator

This repository contains a Rust retirement calculator. Axum serves the Leptos
CSR application and its API, including liveness endpoints at
`GET /api/v1/health/live` and `GET /health`.

The server listens on `0.0.0.0:${PORT}`, with `PORT` defaulting to `8080`.
Supplying a value that is not a valid `u16` causes startup to fail before the
server listens. CORS is intentionally disabled.

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
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test --manifest-path crates/domain/Cargo.toml
bash scripts/check-dependencies.sh
node --test scripts/check-dependencies.test.mjs
npm ci --include=dev
bash scripts/build-web.sh
cargo build --locked --release -p server
cargo run --locked -p server
```

With the server running in another terminal, run `npm run smoke`. The browser
test opens `http://127.0.0.1:8080`, exercises the calculator, and verifies the
versioned WASM client and API behavior.

## Run with Docker

Build the production image and publish the application on a local-only host
port:

```sh
docker build --tag my-retirement-calculator:local .
docker run --rm --name my-retirement-calculator \
  --publish 127.0.0.1:8080:8080 \
  my-retirement-calculator:local
```

Open <http://127.0.0.1:8080/>. The container health endpoint is
<http://127.0.0.1:8080/health>.

The runtime image uses an unprivileged `app` user, contains a locked release
server plus freshly rebuilt web assets, and uses `/app/public` as its asset
directory.

## Production delivery

The production image is
`ghcr.io/joshwood/my-retirement-calculator:<full-git-sha>`. Deployments from
`main` also update the convenience `latest` tag, while Hostinger receives the
immutable full commit SHA through `IMAGE_TAG`. Production is available at
<https://my-retirement-calculator.srv2019569.hstgr.cloud/> and is verified at
<https://my-retirement-calculator.srv2019569.hstgr.cloud/health> after a deploy.

The repository owner must perform this one-time configuration:

1. Create the GitHub Actions secret `HOSTINGER_API_KEY` with a Hostinger API
   token that can deploy to the target VPS.
2. Create the GitHub Actions variable `HOSTINGER_VM_ID` with value `2019569`.
3. Make the GHCR package public if the Hostinger host is not configured to
   authenticate to GHCR.

Never commit either the API token or a substituted secret value. The workflow
uses the repository-provided `GITHUB_TOKEN` only to publish the image.

To roll back, rerun the Hostinger compose deployment with `IMAGE_TAG` set to the
full SHA of a previously published image. Do not use `latest` for rollback,
because it is mutable.

Plan data currently lives only in process memory. Restarting the container,
deploying a new image, or rolling back loses all saved plans.
