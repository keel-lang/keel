# Keel development recipes. Run `just` to list commands.

alias b := build
alias c := check
alias t := test
alias f := fmt
alias l := lint
alias r := run-example

default:
    @just --list

check: (fmt "check") lint doc build test
    @echo "All checks passed"

build mode="release":
    @if [ "{{mode}}" = "debug" ]; then \
        cargo build; \
    else \
        cargo build --release; \
    fi

fmt mode="fmt":
    @if [ "{{mode}}" = "check" ]; then \
        cargo fmt -- --check; \
    else \
        cargo fmt; \
    fi

lint mode="check":
    @if [ "{{mode}}" = "fix" ]; then \
        cargo clippy --fix --allow-dirty; \
    else \
        cargo clippy --all-targets --all-features -- -D warnings; \
    fi

test target="all":
    @target="{{target}}"; \
    if [ "$target" = "unit" ]; then \
        cargo test --lib; \
    elif [ "$target" = "integration" ]; then \
        cargo test --test '*'; \
    elif [ "$target" = "all" ]; then \
        cargo test; \
    else \
        echo "unknown test target: $target"; \
        echo "use: just test [all|unit|integration]"; \
        echo "or:  just test-filter <name>"; \
        exit 1; \
    fi

test-filter filter:
    cargo test {{quote(filter)}}

doc:
    cargo doc --no-deps --document-private-items

docs mode="serve":
    @if [ "{{mode}}" = "build" ]; then \
        cd docs && mdbook build; \
    else \
        cd docs && mdbook serve; \
    fi

run file:
    cargo run -- run {{quote(file)}}

check-file file:
    cargo run -- check {{quote(file)}}

run-example name:
    KEEL_LLM=mock KEEL_ONESHOT=1 cargo run -- run {{quote("examples/" + name + ".keel")}}

hello:
    @just run-example hello_world

run-all-examples:
    @scripts/run-all-examples.sh

repl:
    cargo run -- repl

cov mode="text":
    @command -v cargo-llvm-cov >/dev/null 2>&1 || { echo "cargo-llvm-cov not installed. Run: cargo install cargo-llvm-cov"; exit 1; }
    @if [ "{{mode}}" = "html" ]; then \
        cargo coverage-html; \
    else \
        cargo coverage; \
    fi

clean:
    cargo clean
