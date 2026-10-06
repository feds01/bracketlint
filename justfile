# Bracketlint development commands. Run `just` to list them.

set positional-arguments

# List available recipes
default:
    @just --list --unsorted

# Install the pinned toolchain and the pre-commit hooks
setup:
    rustup toolchain install
    @command -v pre-commit >/dev/null || { echo "pre-commit not found, install it with 'brew install pre-commit' or 'pipx install pre-commit'"; exit 1; }
    pre-commit install

# Install the `bracketlint` binary from this checkout into ~/.cargo/bin
install:
    cargo install --locked --path crates/bl

# Remove the installed `bracketlint` binary
uninstall:
    cargo uninstall bracketlint

# Build the workspace
build:
    cargo build --workspace

# Build an optimised `bracketlint` binary into target/release
build-release:
    cargo build --release -p bracketlint

# Run the CLI from source in the current directory, e.g. `just run check templates/`
[no-cd]
run *args:
    cargo run -q -p bracketlint -- "$@"

# Run all tests, extra args go to `cargo test`, e.g. `just test -p bl_parse`
test *args:
    cargo test "$@"

# Run only the UI snapshot tests in tests/cases, optionally filtered by name
test-ui *filter:
    cargo test -p bl_tests -- "$@"

# Re-generate the `.stderr`/`.stdout` snapshots in tests/cases from current output
update-snapshots *filter:
    REGENERATE_OUTPUT=true cargo test -p bl_tests -- --skip ensure_regenerate_output_is_disabled "$@"

# Run clippy, denying warnings
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without changing files
fmt-check:
    cargo fmt --all -- --check

# Run clippy and the formatting check
lint: clippy fmt-check

# Run everything CI runs: tests, clippy and the formatting check
ci: test lint

# Build and open the API docs for the workspace crates
docs:
    cargo doc --workspace --no-deps --open

# Remove build artifacts
clean:
    cargo clean

# Tag and push a release for the version in crates/bl/Cargo.toml, which triggers the release workflow
release:
    #!/usr/bin/env bash
    set -euo pipefail

    version=$(grep -m1 '^version' crates/bl/Cargo.toml | cut -d'"' -f2)

    if [[ $(git branch --show-current) != main ]]; then
        echo "error: releases are cut from main"
        exit 1
    fi

    if [[ -n $(git status --porcelain) ]]; then
        echo "error: commit or stash your changes first"
        exit 1
    fi

    git fetch --tags origin main
    if [[ $(git rev-parse HEAD) != $(git rev-parse origin/main) ]]; then
        echo "error: local main is not the same as origin/main, pull or push first"
        exit 1
    fi

    read -p "Tag and push v$version? [y/N] " answer
    if [[ $answer != [yY] ]]; then
        exit 0
    fi

    git tag -a "v$version" -m "Release v$version"
    git push origin "v$version"
