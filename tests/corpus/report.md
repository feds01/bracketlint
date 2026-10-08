# Corpus report

How bracketlint does on the real-world templates of each source in `sources.toml`. A template passes if it parses, formats without errors, keeps its tags and formats the same way twice. `just update-snapshots` records this report again.

| Dialect | Templates | Passing | Parse errors | Format errors | Changed tags | Unstable |
|---|--:|--:|--:|--:|--:|--:|
| django | 112 | 82 (73%) | 2 | 20 | 0 | 8 |
| jinja | 123 | 47 (38%) | 71 | 3 | 0 | 2 |
| liquid | 99 | 27 (27%) | 21 | 25 | 0 | 26 |
| twig | 96 | 10 (10%) | 85 | 1 | 0 | 0 |

| Source | Dialect | Templates | Passing | Parse errors | Format errors | Changed tags | Unstable |
|---|---|--:|--:|--:|--:|--:|--:|
| django | django | 112 | 82 (73%) | 2 | 20 | 0 | 8 |
| sphinx | jinja | 17 | 1 (5%) | 14 | 2 | 0 | 0 |
| nbconvert | jinja | 15 | 1 (6%) | 12 | 1 | 0 | 1 |
| symfony | twig | 64 | 8 (12%) | 55 | 1 | 0 | 0 |
| symfony-demo | twig | 32 | 2 (6%) | 30 | 0 | 0 | 0 |
| govuk-frontend | jinja | 82 | 40 (48%) | 42 | 0 | 0 | 0 |
| eleventy-base-blog | jinja | 9 | 5 (55%) | 3 | 0 | 0 | 1 |
| dawn | liquid | 87 | 21 (24%) | 19 | 23 | 0 | 24 |
| minima | liquid | 12 | 6 (50%) | 2 | 2 | 0 | 2 |

## Most common errors

### django

| Templates | First error |
|--:|---|
| 19 | An error occurred when formatting `HTML` |
| 1 | An error occurred when parsing the `CSS`. |
| 1 | unexpectedly encountered a `(...)` |
| 1 | unexpectedly encountered an identifier |

### jinja

| Templates | First error |
|--:|---|
| 59 | unexpectedly encountered a `(...)` |
| 3 | unexpectedly encountered an identifier |
| 2 | An error occurred when formatting `HTML` |
| 2 | unexpectedly encountered a `:` |
| 2 | unexpectedly encountered a `[...]` |
| 2 | unexpectedly encountered the keyword `if` |
| 1 | An error occurred when parsing the `JavaScript`. |
| 1 | unexpectedly encountered a `+` |
| 1 | unexpectedly encountered a `=` |
| 1 | unexpectedly encountered the keyword `or` |

### liquid

| Templates | First error |
|--:|---|
| 18 | An error occurred when formatting `HTML` |
| 12 | unexpectedly encountered an unknown character `?` |
| 6 | expected a tag, however received an unknown character `#` |
| 5 | An error occurred when parsing the `CSS`. |
| 2 | An error occurred when parsing the `JavaScript`. |
| 2 | unexpectedly encountered the keyword `include` |
| 1 | unexpectedly encountered a `}}` |

### twig

| Templates | First error |
|--:|---|
| 49 | unexpectedly encountered a `(...)` |
| 17 | unexpectedly encountered a string literal |
| 5 | unexpectedly encountered the keyword `include` |
| 4 | encountered unclosed delimiter `(`, add a `)` after the inner expression |
| 4 | encountered unclosed delimiter `{%`, add a `%}` after the inner expression |
| 2 | unexpectedly encountered an unknown character `{` |
| 2 | unexpectedly encountered the keyword `raw` |
| 1 | An error occurred when formatting `HTML` |
| 1 | unexpectedly encountered a `[...]` |
| 1 | unexpectedly encountered an unknown character `?` |
