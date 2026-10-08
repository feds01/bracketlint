# Benchmark templates

Real-world templates that the benchmarks lex and parse, copied unchanged from
their projects. Each is under its project's licence, which is in `licenses/`.

| Template | Source | Licence |
|---|---|---|
| `django_technical_500.html` | [`django/views/templates/technical_500.html`](https://github.com/django/django/blob/6.1.2/django/views/templates/technical_500.html) from Django 6.1.2 | BSD-3-Clause, [`licenses/django.txt`](licenses/django.txt) |
| `minimal_mistakes_single.liquid` | [`_layouts/single.html`](https://github.com/mmistakes/minimal-mistakes/blob/4.28.1/_layouts/single.html) from Minimal Mistakes 4.28.1 | MIT, [`licenses/minimal-mistakes.txt`](licenses/minimal-mistakes.txt) |

A template must parse without errors, or it would only benchmark the error
path, which `cargo test -p bl_benchmark` checks.
