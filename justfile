# Common development tasks for rune-term.

default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy -p term-grid -p term-input -p term-control -p rune-term-ctl --all-targets -- -D warnings
    cargo clippy -p rune-term --all-targets --no-default-features --features graphics -- -D warnings

check:
    cargo check --workspace --all-targets

check-lib:
    cargo check -p rune-term --no-default-features --lib
    cargo check -p term-control -p rune-term-ctl

test-lib:
    cargo test -p term-grid --lib
    cargo test -p term-input --lib
    cargo test -p rune-term --no-default-features --lib
    cargo test -p term-control --lib

test-grid:
    cargo test -p rune-term --test grid_pipeline

test-headless:
    cargo test -p rune-term --no-default-features --test headless

test: test-lib

# Pre-commit gate: formatting, clippy (no warnings), and lib tests.
check-all: fmt-check clippy test-lib

run:
    cargo run -p rune-term

ci: check-all

ctl *args:
    cargo run -p rune-term-ctl -- {{args}}
