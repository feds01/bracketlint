# Bracketlint development commands. Run `just` to list them.

set positional-arguments

# The version of the `bracketlint` binary, which a release is of
bracketlint_version := `sed -n 's/^version = "\(.*\)"/\1/p' crates/bl/Cargo.toml | head -1 | grep .`

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

# Run only the UI snapshot tests in tests/cases and the corpus in tests/corpus, optionally filtered by name
test-ui *filter:
    cargo test -p bl_tests -- "$@"

# Re-generate the `.stderr`/`.stdout` snapshots in tests/cases, and the baselines in tests/corpus, from current output
update-snapshots *filter:
    REGENERATE_OUTPUT=true cargo test -p bl_tests -- --skip ensure_regenerate_output_is_disabled "$@"

# Check the real-world templates in tests/corpus, and print its report. `just test` runs this too
corpus:
    cargo test -p bl_tests -- corpus --nocapture

# Compare how the templates in tests/corpus do with the baselines at a git ref, as CI comments on pull requests, e.g. `just corpus-compare origin/main`
corpus-compare ref="main":
    #!/usr/bin/env bash
    set -euo pipefail

    base=$(mktemp -d)
    trap 'rm -rf "$base"' EXIT
    git archive "$1" tests/corpus/baselines | tar -x -C "$base" --strip-components=3

    # The comparison is written before the baselines are checked, so it is
    # shown even if they are out of date.
    rm -f target/corpus/comparison.md
    CORPUS_COMPARE_WITH="$base" cargo test -q -p bl_tests -- corpus || status=$?
    cat target/corpus/comparison.md
    exit "${status:-0}"

# Run the lexer, parser and formatter benchmarks, extra args go to divan, e.g. `just bench django`
bench *args:
    cargo bench -p bl_benchmark -- "$@"

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

# Write CHANGELOG.md with a section for the release `version`, e.g. `just changelog 0.5.0`: the pull requests merged since the last tag, by their titles
changelog version:
    @command -v git-cliff >/dev/null || { echo "git-cliff not found, install it with 'brew install git-cliff' or 'cargo install git-cliff'"; exit 1; }
    git cliff --tag "v{{version}}" --output CHANGELOG.md

# Tag and push a release from main, using the version in crates/bl/Cargo.toml
release:
    #!/usr/bin/env bash
    set -euo pipefail

    fail() {
        echo "error: $1" >&2
        exit 1
    }

    # Releases are cut from an up-to-date, clean main.
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [ "$branch" != "main" ]; then
        fail "releases are cut from main (on '$branch')"
    fi
    if [ -n "$(git status --porcelain)" ]; then
        fail "the working tree is not clean"
    fi
    git fetch origin main --tags --quiet
    if [ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]; then
        fail "main is not in sync with origin/main"
    fi

    # The version comes from the binary's crate, and must not be tagged yet.
    version="$(sed -n 's/^version = "\(.*\)"/\1/p' crates/bl/Cargo.toml | head -1)"
    tag="v$version"
    if git rev-parse --quiet --verify "refs/tags/$tag" >/dev/null; then
        fail "tag $tag already exists, bump the version first"
    fi

    # dist takes the release's notes from its section of the changelog.
    if ! grep -q "^## \[$version\]" CHANGELOG.md; then
        fail "CHANGELOG.md has no section for $version: run 'just changelog $version' and merge it"
    fi

    read -r -p "Release $tag? [y/N] " answer
    case "$answer" in
        y | Y) ;;
        *)
            echo "aborted"
            exit 1
            ;;
    esac

    git tag -a "$tag" -m "Release $tag"
    git push origin "$tag"
    echo "Once the release workflow has run, approve the npm packages on npmjs.com, under Staged Packages."

# Commit the Homebrew formula of each release in $PLAN, dist's plan, to the tap checked out at `tap`, and push it (run by the release workflow)
[no-cd]
publish-formula tap:
    #!/usr/bin/env bash
    set -euo pipefail
    cd "{{tap}}"
    git config user.name "github-actions[bot]"
    git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
    export PATH="/home/linuxbrew/.linuxbrew/bin:$PATH"
    brew update

    # Each release with a formula, as a line of JSON. Its formula may be named
    # other than the app.
    releases="$(jq --compact-output '.releases[] | select(any(.artifacts[]; endswith(".rb")))' <<<"$PLAN")"
    while IFS= read -r release; do
        [ -n "$release" ] || continue
        formula="$(jq --raw-output '.artifacts[] | select(endswith(".rb"))' <<<"$release")"
        version="$(jq --raw-output '.app_version' <<<"$release")"

        # Fix the formula's style, without reformatting its description or homepage.
        brew style --except-cops FormulaAudit/Homepage,FormulaAudit/Desc,FormulaAuditStrict \
            --fix "Formula/$formula" || true

        git add "Formula/$formula"
        git commit --message "${formula%.rb} $version"
    done <<<"$releases"

    git push

# Assemble and pack the npm packages into `out`, from the release archives in `archives`, at the binary's version unless given another, and only for `targets` if given any
[no-cd]
npm-packages archives out version="" *targets:
    #!/usr/bin/env bash
    set -euo pipefail
    shift 3
    # An absolute path: npm reads a relative one, like `out/bracketlint`, as a
    # GitHub repository.
    out="$(mkdir -p "{{out}}" && cd "{{out}}" && pwd)"
    node "{{justfile_directory()}}/npm/assemble.mjs" "{{ if version == "" { bracketlint_version } else { version } }}" "{{archives}}" "$out" "$@"
    for package in "$out"/bracketlint*/; do
        npm pack --json --pack-destination "$out" "$package" | jq --raw-output '.[].filename'
    done

# Stage the npm packages of the release, at the binary's version, from the archives in `dir`, for a maintainer to approve, skipping versions already on npm (run by the release workflow)
[no-cd]
publish-npm dir:
    #!/usr/bin/env bash
    set -euo pipefail
    # 1. Assemble the packages: one per platform, and @feds01/bracketlint.
    out="$(mktemp -d)"
    node "{{justfile_directory()}}/npm/assemble.mjs" "{{bracketlint_version}}" "{{dir}}" "$out"

    # 2. Stage the platform packages first, then @feds01/bracketlint, which
    #    depends on them.
    for package in "$out"/bracketlint-*/ "$out/bracketlint/"; do
        # What it stages, e.g. `@feds01/bracketlint@0.3.0`.
        spec="$(jq --raw-output '.name + "@" + .version' "$package/package.json")"

        # A version already on npm, published by hand or approved after an
        # earlier run, is skipped, so that the job can be re-run. One that is
        # staged but not yet approved can't be staged again: approve or reject
        # it first.
        if [ -n "$(npm view "$spec" version 2>/dev/null)" ]; then
            echo "$spec is already on npm"
            continue
        fi

        # Staged, not published: it goes live once a maintainer approves it,
        # with 2FA. npm prints the id to approve it by, as
        # `+ @feds01/bracketlint@0.3.0 (staged with id <id>)`.
        tarball="$out/$(npm pack --json --pack-destination "$out" "$package" | jq --raw-output '.[].filename')"
        staged="$(npm stage publish --access public "$tarball")"
        echo "$staged"
        id="$(sed -n 's/.*(staged with id \(.*\))$/\1/p' <<<"$staged")"
        echo "::notice title=npm::Staged $spec. Approve it on npmjs.com, under Staged Packages, or with npm stage approve ${id:-<id>}."
    done

