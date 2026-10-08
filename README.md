# Bracketlint :zap:

> HTML template linter for [Jinja](https://jinja.palletsprojects.com/), [Nunjucks](https://mozilla.github.io/nunjucks/), [Django templates](https://docs.djangoproject.com/en/dev/topics/templates/), [Twig](https://twig.symfony.com/), [Liquid](https://shopify.github.io/liquid/).

## Installation

Prebuilt binaries for macOS (Apple Silicon, Intel), Linux (x86_64, arm64; statically linked) and Windows ship with every [release](https://github.com/feds01/bracketlint/releases). No Rust toolchain needed.

macOS / Linux:

```bash
$ curl --proto '=https' --tlsv1.2 -LsSf https://github.com/feds01/bracketlint/releases/latest/download/bracketlint-installer.sh | sh
```

Windows:

```powershell
> powershell -ExecutionPolicy Bypass -c "irm https://github.com/feds01/bracketlint/releases/latest/download/bracketlint-installer.ps1 | iex"
```

The installer puts `bracketlint` in `~/.local/bin` (or `$XDG_BIN_HOME`). To fetch an archive directly instead, e.g. in CI:

```bash
$ gh release download --repo feds01/bracketlint --pattern 'bracketlint-aarch64-apple-darwin.tar.xz'
```

From source (with Rust & Cargo installed):

```bash
$ cargo install --git https://github.com/feds01/bracketlint bracketlint
```

### Development

Common tasks are recipes in the [`justfile`](./justfile) (install [just](https://github.com/casey/just) with `brew install just` or `cargo install just`). Run `just` to list them all:

```bash
$ just setup             # install the pinned nightly toolchain and pre-commit hooks
$ just install           # install `bracketlint` from this checkout into ~/.cargo/bin
$ just run check foo/    # run the CLI from source
$ just test              # run all tests
$ just update-snapshots  # re-generate the UI test snapshots in tests/cases, and how the real-world templates in tests/corpus do
$ just corpus            # print how the real-world templates in tests/corpus do
$ just ci                # run tests, clippy and the formatting check, like CI does
$ just bench             # run the lexer and parser benchmarks
```

The benchmarks in [`crates/bl_benchmark`](./crates/bl_benchmark) use [divan](https://github.com/nvzqz/divan). In CI, [CodSpeed](https://codspeed.io) also runs them on every pull request and reports any regressions against `main`.

### Releasing

Releases are built by [dist](https://github.com/axodotdev/cargo-dist) (`.github/workflows/release.yml`, config in `dist-workspace.toml`). Bump `version` in `crates/bl/Cargo.toml`, merge, then from an up-to-date `main` push a matching tag:

```bash
$ just release   # tags v<version> from crates/bl/Cargo.toml and pushes it
```
