FROM node:24.21.0-bookworm-slim AS node-toolchain

FROM rust:1.88-bookworm AS builder

COPY --from=node-toolchain /usr/local/ /usr/local/

RUN rustup target add wasm32-unknown-unknown \
    && cargo install wasm-bindgen-cli --version 0.2.100 --locked

WORKDIR /build
COPY . .
RUN npm ci --include=dev \
    && npm run verify:stage1
RUN bash scripts/build-web.sh
RUN cargo build --locked --release -p server

FROM debian:bookworm-slim AS runtime

RUN groupadd --system app \
    && useradd --system --gid app --home-dir /app --no-create-home app

WORKDIR /app
COPY --from=builder --chown=app:app /build/target/release/server /app/server
COPY --from=builder --chown=app:app /build/public /app/public

ENV RETIREMENT_CALCULATOR_PUBLIC_DIR=/app/public
EXPOSE 8080
USER app
ENTRYPOINT ["/app/server"]
