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

### Releasing

Releases are built by [dist](https://github.com/axodotdev/cargo-dist) (`.github/workflows/release.yml`, config in `dist-workspace.toml`). Bump `version` in `crates/bl/Cargo.toml`, merge, then push a matching tag:

```bash
$ git tag v0.2.1 && git push origin v0.2.1
```
