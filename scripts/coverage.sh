#!/usr/bin/env bash
set -euo pipefail

cargo llvm-cov --workspace --all-targets --summary-only
