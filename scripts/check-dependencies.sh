#!/usr/bin/env bash
set -euo pipefail

cargo metadata --format-version 1 --no-deps | node scripts/check-dependencies.mjs
