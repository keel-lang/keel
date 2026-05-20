#!/usr/bin/env bash
# Run runnable .keel examples as a smoke test. Exits on first failure.
set -euo pipefail

cd "$(dirname "$0")/.."  # cd to repo root from scripts/

export KEEL_LLM="${KEEL_LLM:-mock}"
export KEEL_ONESHOT="${KEEL_ONESHOT:-1}"

count=0
skipped=0

should_skip() {
    case "$1" in
        examples/capability_gating_fail.keel|\
        examples/http_demo.keel|\
        examples/webhook_agent.keel)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

for f in examples/*.keel; do
    if should_skip "$f"; then
        echo "SKIP: $f"
        ((skipped += 1))
        continue
    fi

    echo "==> $f"
    if cargo run -- run "$f"; then
        ((count += 1))
    else
        echo "FAILED: $f"
        exit 1
    fi
done

echo "All $count runnable examples ran successfully ($skipped skipped)"
