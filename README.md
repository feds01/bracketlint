# Bracketlint :zap:

[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/feds01/bracketlint?utm_source=badge)

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

### Usage

```bash
$ bracketlint check templates/        # report problems in the templates
$ bracketlint fmt templates/          # format the templates in place
$ bracketlint fmt --check templates/  # list the templates that formatting would change
$ bracketlint fmt --diff templates/   # print a diff of how formatting would change them
```

`fmt --check` and `fmt --diff` don't write any file, and exit with 1 if formatting would change a template, so CI can require formatted templates. The list and the diff go to stdout, and everything else to stderr, so `bracketlint fmt --diff templates/ > changes.diff` writes a plain diff that `patch -p0` applies.

Files that `.gitignore` and other ignore files exclude are skipped, unless you pass `--no-respect-gitignore`. Each file is read in the dialect that its extension names: `.jinja` is Jinja, `.liquid` is Liquid, `.njk` is Nunjucks and `.twig` is Twig. Any other file, such as `.html`, is read as Django, unless the project's [`bracketlint.toml`](#configuration) sets another dialect. To read every file in one dialect instead, pass `--dialect`:

```bash
$ bracketlint check --dialect liquid theme/
```

Only errors fail a run. A warning, such as another dialect's spelling of a tag, is reported but doesn't fail it, unless you pass `--deny-warnings`:

```bash
$ bracketlint check --deny-warnings templates/
```

`bracketlint` exits with 0 when a run passes, 1 when it fails, and 2 when it couldn't run, e.g. when an argument is wrong, or it crashed. Pass `-v` (`--verbose`) to also print debug logs, such as how long each step of a run takes.

### Configuration

A project's settings go in a `bracketlint.toml`, which bracketlint reads from the current directory, or else the closest of its parents that has one:

```toml
# The dialect of the templates whose extension doesn't name one, such as
# `.html`, which are otherwise read as Django.
dialect = "jinja"

# What the templates output: `html`, the default, or `text`, such as
# Markdown, plain-text emails or prompts for a language model.
markup = "text"
```

Whitespace in a `text` template is part of its output, so `bracketlint fmt` keeps all of the text between its tags exactly as it is written, and only formats the tags. In an `html` template, it indents the text and wraps its lines, as HTML shows a run of whitespace as a single space.

### Development

Common tasks are recipes in the [`justfile`](./justfile) (install [just](https://github.com/casey/just) with `brew install just` or `cargo install just`). Run `just` to list them all:

```bash
$ just setup             # install the pinned nightly toolchain and pre-commit hooks
$ just install           # install `bracketlint` from this checkout into ~/.cargo/bin
$ just run check foo/    # run the CLI from source
$ just test              # run all tests
$ just update-snapshots  # re-generate the UI test snapshots in tests/cases, and how the real-world templates in tests/corpus do
$ just corpus            # print how the real-world templates in tests/corpus do
$ just corpus-compare    # compare them with the baselines on main, as the comment on each PR does
$ just ci                # run tests, clippy and the formatting check, like CI does
$ just bench             # run benchmarks
```

### Releasing

Releases are built by [dist](https://github.com/axodotdev/cargo-dist) (`.github/workflows/release.yml`, config in `dist-workspace.toml`). Bump `version` in `crates/bl/Cargo.toml`, merge, then from an up-to-date `main` push a matching tag:

```bash
$ just release   # tags v<version> from crates/bl/Cargo.toml and pushes it
```
