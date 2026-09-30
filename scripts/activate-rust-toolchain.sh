#!/usr/bin/env bash

# Source this file to use the workspace-local Rust/WASM toolchain.
toolchain_workspace_root="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
export RUSTUP_HOME="$toolchain_workspace_root/.toolchain/rustup"
export CARGO_HOME="$toolchain_workspace_root/.toolchain/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
unset toolchain_workspace_root
