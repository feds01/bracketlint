# Contributing

## Development

Bracketlint builds with a pinned nightly toolchain (see `rust-toolchain.toml`), and common tasks are recipes in the [`justfile`](justfile) (install [just](https://github.com/casey/just) with `brew install just` or `cargo install just`). Run `just` to list them all:

```sh
just setup             # install the pinned nightly toolchain and pre-commit hooks
just install           # install `bracketlint` from this checkout into ~/.cargo/bin
just run check foo/    # run the CLI from source
just test              # run all tests
just update-snapshots  # re-generate the UI test snapshots in tests/cases, and how the real-world templates in tests/corpus do
just corpus            # print how the real-world templates in tests/corpus do
just corpus-compare    # compare them with the baselines on main, as the comment on each PR does
just ci                # run tests, clippy and the formatting check, like CI does
just bench             # run benchmarks
```

`just ci` runs what CI runs: the tests, including the UI cases in `tests/cases` and the corpus in `tests/corpus`, clippy and the formatting check.

## Dialect rules link their spec

Bracketlint reads Django, Jinja, Liquid, Nunjucks and Twig templates, which differ in small ways. For example, Liquid spells `elif` as `elsif`, `{# #}` is text in Liquid, and only Jinja, Nunjucks and Twig take operators inside `{{ }}`. Each difference is a rule on `Dialect`, in `crates/bl_ast/src/dialect.rs`. Each rule links the spec it follows, so that a reader can check it against the dialect rather than trust the code.

When you add or change a rule:

- Link its spec on the `Dialect` method, and at each place in the lexer, parser or formatter that checks it. A function that checks a rule more than once links it once.
- Link each dialect that the rule names. A rule that only Liquid has links Liquid.
- Link the dialect's own docs where they state the rule:
  - Django: <https://docs.djangoproject.com/en/stable/ref/templates/>
  - Jinja: <https://jinja.palletsprojects.com/en/stable/templates/>
  - Liquid: <https://shopify.github.io/liquid/>
  - Nunjucks: <https://mozilla.github.io/nunjucks/templating.html>
  - Twig: <https://twig.symfony.com/doc/3.x/>
- Where the docs don't state the rule, link the engine's source at a release tag, with the lines that decide it, e.g. <https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/lexer.rb#L23>. A link to a branch drifts as the branch changes.
- Open every link before you commit it, and check that its anchor or lines exist.

Write the links as Markdown reference links. Name each source in the prose, e.g. `[Jinja]`, and define the label under the prose, after an empty comment line. Rustdoc only renders a definition whose label the prose uses, and then shows a link rather than a bare URL. Label a link with its dialect when the comment has one link per dialect, and more precisely when a dialect has several, e.g. ``[`cycle`]`` and ``[`render`]``.

On the `Dialect` method:

```rust
/// Whether `{% raw %}` blocks exist, as in [Jinja] and [Liquid], whose
/// contents are output as they are written. [Django] and [Twig] have
/// `{% verbatim %}` instead.
///
/// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#escaping
/// [Liquid]: https://shopify.github.io/liquid/tags/template/#raw
/// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#verbatim
/// [Twig]: https://twig.symfony.com/doc/3.x/tags/verbatim.html
pub fn has_raw_blocks(self) -> bool {
```

Where the rule is checked, write `//` comments the same way:

```rust
// The contents of a `{% raw %}` block, as in [Jinja] and [Liquid], are
// output as they are written, so they are text rather than tokens.
//
// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#escaping
// [Liquid]: https://shopify.github.io/liquid/tags/template/#raw
if this.dialect.has_raw_blocks()
```

## Releasing

Bump `version` in `crates/bl/Cargo.toml`, and with it `Cargo.lock`, and run `just changelog <version>`, which writes [CHANGELOG.md](CHANGELOG.md) with [git-cliff](https://git-cliff.org) from the titles of the pull requests merged since the last release (see [cliff.toml](cliff.toml)). Merge that to `main`, then run `just release` to tag it. [dist](https://github.com/axodotdev/cargo-dist) (`.github/workflows/release.yml`, configured in `dist-workspace.toml`) builds the binaries and installers, and publishes the GitHub release, with the release's section of the changelog as its notes, the Homebrew formula, to [feds01/homebrew-tap](https://github.com/feds01/homebrew-tap), and the npm packages. A pre-release, such as `0.4.0-beta.1`, gets only the GitHub release: the formula and the npm packages wait for a release. The GitHub release is published once the formula is pushed and the npm packages are staged, so a release whose npm job fails has no GitHub release until it is re-run.

A pull request's title is its entry in the changelog, so write it as a conventional commit, as in `feat(parse): …`. Features, fixes, performance and documentation are listed; other types are not.

The npm packages are built from the release's archives by `just publish-npm`, from the templates in [npm/](npm): a package per platform with just its binary, such as `@feds01/bracketlint-darwin-arm64`, and `@feds01/bracketlint`, which depends on them all, optionally, and runs the right one. `just npm-packages` builds them locally. They are staged with npm's trusted publishing, without a token, rather than published: approve them on npmjs.com, under Staged Packages, or with `npm stage approve <id>` (npm 11.15 or later), the platform packages before `@feds01/bracketlint`. The release run shows the ids, and so does `npm stage list <package>`. The npm job skips a version that is already on npm, so it can be re-run, but a package that is staged and not yet approved must be approved or rejected first.

npm can only trust the release workflow with a package that exists, so before the first release that publishes them, for `@feds01/bracketlint` and each of its platform packages:

1. Publish a placeholder version, `0.0.0-stage`, with only a `package.json` and a README, by hand: `npm publish --access public --tag stage`.
2. On npmjs.com, under the package's settings, add a trusted publisher: GitHub Actions, the repository `feds01/bracketlint`, and the workflow `release.yml`.
