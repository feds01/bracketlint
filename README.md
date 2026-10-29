# Bracketlint

[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/feds01/bracketlint?utm_source=badge)

> A linter and formatter for Django, Jinja, Liquid, Nunjucks and Twig templates.

`bracketlint check` reports the problems in a project's templates, such as a
tag that is never closed, or another dialect's spelling of a tag.
`bracketlint fmt` formats them: it lays out their HTML and their tags, and
evens out the spacing inside each tag. It reads [Django](https://docs.djangoproject.com/en/stable/ref/templates/),
[Jinja](https://jinja.palletsprojects.com/en/stable/templates/),
[Liquid](https://shopify.github.io/liquid/),
[Nunjucks](https://mozilla.github.io/nunjucks/templating.html) and
[Twig](https://twig.symfony.com/doc/3.x/) templates.

## Installation

See [INSTALL.md](INSTALL.md). With npm, for example:

```sh
npm install --save-dev @feds01/bracketlint
```

## Usage

```sh
bracketlint check templates/        # report problems in the templates
bracketlint fmt templates/          # format the templates in place
bracketlint fmt --check templates/  # list the templates that formatting would change
bracketlint fmt --diff templates/   # print a diff of how formatting would change them
```

`fmt --check` and `fmt --diff` don't write any file, and exit with 1 if
formatting would change a template, so CI can require formatted templates.
The list and the diff go to stdout, and everything else to stderr, so
`bracketlint fmt --diff templates/ > changes.diff` writes a plain diff that
`patch -p0` applies.

Files that `.gitignore` and other ignore files exclude are skipped, unless you
pass `--no-respect-gitignore`. Each file is read in the dialect that its
extension names: `.jinja` is Jinja, `.liquid` is Liquid, `.njk` is Nunjucks
and `.twig` is Twig. Any other file, such as `.html`, is read as Django, unless
the project's [`bracketlint.toml`](#configuration) sets another dialect. To
read every file in one dialect instead, pass `--dialect`:

```sh
bracketlint check --dialect liquid theme/
```

Only errors fail a run. A warning, such as another dialect's spelling of a
tag, is reported but doesn't fail it, unless you pass `--deny-warnings`:

```sh
bracketlint check --deny-warnings templates/
```

`bracketlint` exits with 0 when a run passes, 1 when it fails, and 2 when it
couldn't run, e.g. when an argument is wrong, or it crashed. Pass `-v`
(`--verbose`) to also print debug logs, such as how long each step of a run
takes.

## Configuration

A project's settings go in a `bracketlint.toml`, which bracketlint reads from
the current directory, or else the closest of its parents that has one:

```toml
# The dialect of the templates whose extension doesn't name one, such as
# `.html`, which are otherwise read as Django.
dialect = "jinja"

# What the templates output: `html`, the default, or `text`, such as
# Markdown, plain-text emails or prompts for a language model.
markup = "text"
```

Whitespace in a `text` template is part of its output, so `bracketlint fmt`
keeps all of the text between its tags exactly as it is written, and only
formats the tags. In an `html` template, it indents the text and wraps its
lines, as HTML shows a run of whitespace as a single space.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT
