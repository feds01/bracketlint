# Contributing

The [README](README.md#development) lists the `just` recipes for building and testing a change. `just ci` runs what CI runs: the tests, including the UI cases in `tests/cases` and the corpus in `tests/corpus`, clippy and the formatting check.

## Dialect rules link their spec

Bracketlint reads Django, Jinja, Liquid and Twig templates, which differ in small ways. For example, Liquid spells `elif` as `elsif`, `{# #}` is text in Liquid, and only Jinja and Twig take operators inside `{{ }}`. Each difference is a rule on `Dialect`, in `crates/bl_ast/src/dialect.rs`. Each rule links the spec it follows, so that a reader can check it against the dialect rather than trust the code.

When you add or change a rule:

- Link its spec on the `Dialect` method, and at each place in the lexer, parser or formatter that checks it. A function that checks a rule more than once links it once.
- Link each dialect that the rule names. A rule that only Liquid has links Liquid.
- Link the dialect's own docs where they state the rule:
  - Django: <https://docs.djangoproject.com/en/stable/ref/templates/>
  - Jinja: <https://jinja.palletsprojects.com/en/stable/templates/>
  - Liquid: <https://shopify.github.io/liquid/>
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
