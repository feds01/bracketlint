# Changelog

What changed for bracketlint's users in each release. dist shows a release's
section as its notes on GitHub.

## [0.3.0] - 2026-10-29

[Changes since v0.2.0](https://github.com/feds01/bracketlint/compare/v0.2.0...v0.3.0)

### Features

- **release:** install with Homebrew, with `brew install feds01/tap/bracketlint` ([#134](https://github.com/feds01/bracketlint/pull/134))
- **release:** install with npm, with `npm install --save-dev @feds01/bracketlint` ([#145](https://github.com/feds01/bracketlint/pull/145))

### Fixes

- **parse:** read a conditional loop iterator in Nunjucks and Twig, e.g. `{% for x in a ? b : c %}` ([#136](https://github.com/feds01/bracketlint/pull/136))

### Performance

- lex and parse with fewer instructions and allocations ([#137](https://github.com/feds01/bracketlint/pull/137))
- **ast:** keep the largest tags and expressions behind a node ([#138](https://github.com/feds01/bracketlint/pull/138))
- **lexer:** lex the tokens of a tree in its loop, rather than with a call for each ([#140](https://github.com/feds01/bracketlint/pull/140))
- **cli:** read, parse and format files in parallel in check and fmt ([#141](https://github.com/feds01/bracketlint/pull/141))
- **lexer:** inline skipping whitespace, and the parser's checks of a dialect's syntax ([#142](https://github.com/feds01/bracketlint/pull/142))
- **cli:** read the current directory once, rather than for each file named ([#143](https://github.com/feds01/bracketlint/pull/143))

## [0.2.0] - 2026-10-13

[Changes since v0.1.0](https://github.com/feds01/bracketlint/compare/v0.1.0...v0.2.0)

### Features

- pick the template dialect per file ([#22](https://github.com/feds01/bracketlint/pull/22))
- **parse:** support whitespace control ([#25](https://github.com/feds01/bracketlint/pull/25))
- **liquid:** parse Liquid expressions ([#26](https://github.com/feds01/bracketlint/pull/26))
- **liquid:** parse Liquid control flow ([#27](https://github.com/feds01/bracketlint/pull/27))
- **liquid:** parse capture, render and Liquid tag arguments ([#28](https://github.com/feds01/bracketlint/pull/28))
- **liquid:** parse liquid tags and inline comments, and find .liquid files ([#29](https://github.com/feds01/bracketlint/pull/29))
- **parse:** parse arithmetic and `~` operators in Jinja and Twig ([#53](https://github.com/feds01/bracketlint/pull/53))
- **liquid:** parse `{% doc %}` blocks, and `include` as a name ([#75](https://github.com/feds01/bracketlint/pull/75))
- **parse:** say when a parse error comes from another dialect's syntax ([#83](https://github.com/feds01/bracketlint/pull/83))
- **parse:** parse calls, groups, tuples and lists in Jinja and Twig ([#84](https://github.com/feds01/bracketlint/pull/84))
- **parse:** parse mappings in Jinja and Twig ([#85](https://github.com/feds01/bracketlint/pull/85))
- **parse:** parse conditional expressions in Jinja and Twig ([#86](https://github.com/feds01/bracketlint/pull/86))
- **parse:** parse the rest of Twig's expressions and tags ([#87](https://github.com/feds01/bracketlint/pull/87))
- **parse:** parse Jinja's block modifiers and slices, and keyword attributes ([#88](https://github.com/feds01/bracketlint/pull/88))
- add the Nunjucks dialect ([#98](https://github.com/feds01/bracketlint/pull/98))
- **parse:** parse Nunjucks' strict comparisons and name keys ([#99](https://github.com/feds01/bracketlint/pull/99))
- **parse:** parse the template that a tag names as an expression ([#107](https://github.com/feds01/bracketlint/pull/107))
- **parse:** parse `with context` and `without context` on imports ([#108](https://github.com/feds01/bracketlint/pull/108))
- **parse:** parse Jinja's `*args` and `**kwargs` in calls ([#109](https://github.com/feds01/bracketlint/pull/109))
- **parse:** parse Jinja's loop filters and `with` values as expressions ([#110](https://github.com/feds01/bracketlint/pull/110))
- **parse:** parse Jinja tests whose argument has no parentheses ([#111](https://github.com/feds01/bracketlint/pull/111))
- **parse:** parse Jinja's adjacent strings and tuples in `{{ }}` ([#112](https://github.com/feds01/bracketlint/pull/112))
- **parse:** parse Jinja keywords as names in expressions ([#113](https://github.com/feds01/bracketlint/pull/113))
- **parse:** parse a Django block's name as any word ([#114](https://github.com/feds01/bracketlint/pull/114))
- **parse:** parse the rest of Twig's binary operators ([#115](https://github.com/feds01/bracketlint/pull/115))
- **parse:** parse Twig keywords as names in expressions ([#116](https://github.com/feds01/bracketlint/pull/116))
- **parse:** parse Nunjucks groups of several expressions ([#117](https://github.com/feds01/bracketlint/pull/117))
- **parse:** parse Twig's guard blocks ([#118](https://github.com/feds01/bracketlint/pull/118))
- **parse:** parse Jekyll's `include` and `include_relative` in Liquid ([#122](https://github.com/feds01/bracketlint/pull/122))
- **parse:** keep Liquid markup that doesn't parse as written, with a warning ([#123](https://github.com/feds01/bracketlint/pull/123))
- **parse:** accept text after `else` and end tags in Liquid ([#125](https://github.com/feds01/bracketlint/pull/125))
- **fmt:** format whole `<script>` and `<style>` bodies, and report those that don't parse ([#127](https://github.com/feds01/bracketlint/pull/127))
- **fmt:** format templates that output text, e.g. Markdown prompts ([#129](https://github.com/feds01/bracketlint/pull/129))

### Fixes

- **fmt:** indent every line of formatted text ([#17](https://github.com/feds01/bracketlint/pull/17))
- **fmt:** keep spaces around inline tags ([#18](https://github.com/feds01/bracketlint/pull/18))
- **fmt:** stop dropping template source ([#23](https://github.com/feds01/bracketlint/pull/23))
- **parse:** parse raw blocks, filter chains, `{#` in Liquid and unclosed ifs ([#24](https://github.com/feds01/bracketlint/pull/24))
- **fmt:** keep tags that are kept as written on their own lines ([#48](https://github.com/feds01/bracketlint/pull/48))
- **parse:** parse `is not` and `not in` as their own operators ([#54](https://github.com/feds01/bracketlint/pull/54))
- **parse:** group comparison operators the way each dialect does ([#55](https://github.com/feds01/bracketlint/pull/55))
- **lexer:** let a Liquid name end with `?` ([#57](https://github.com/feds01/bracketlint/pull/57))
- **parse:** parse binary operators inside `{{ }}` in Jinja and Twig ([#64](https://github.com/feds01/bracketlint/pull/64))
- **corpus:** download a checkout again when CI's cache emptied it ([#79](https://github.com/feds01/bracketlint/pull/79))
- **fmt:** keep the pieces of a template that don't parse as they are written ([#76](https://github.com/feds01/bracketlint/pull/76))
- **fmt:** indent by the HTML elements that are open, rather than guessing ([#82](https://github.com/feds01/bracketlint/pull/82))
- **fmt:** end lines without the spaces at their end ([#90](https://github.com/feds01/bracketlint/pull/90))
- **fmt:** format text around an inline element the same way twice ([#96](https://github.com/feds01/bracketlint/pull/96))
- **ast:** put the line ending of a `\r\n` line on that line ([#97](https://github.com/feds01/bracketlint/pull/97))
- **parse:** read a `{# #}` comment after text as a comment ([#119](https://github.com/feds01/bracketlint/pull/119))
- **fmt:** format comments and attribute values across lines the same way twice ([#120](https://github.com/feds01/bracketlint/pull/120))
- **fmt:** format a block whose branches open a `<style>` the same way twice ([#121](https://github.com/feds01/bracketlint/pull/121))
- **cli:** get the CLI ready for its first release ([#128](https://github.com/feds01/bracketlint/pull/128))
- **fmt:** wrap the lines of text with the `{{ }}` tags in them ([#130](https://github.com/feds01/bracketlint/pull/130))
- **fmt:** keep blocks of text to translate as they are written ([#131](https://github.com/feds01/bracketlint/pull/131))

### Performance

- **lexer:** reserve space for the tokens up front ([#46](https://github.com/feds01/bracketlint/pull/46))
- **lexer:** find the end of text with memchr ([#47](https://github.com/feds01/bracketlint/pull/47))
- **lexer:** scan identifiers as bytes ([#61](https://github.com/feds01/bracketlint/pull/61))
- **lexer:** skip ASCII whitespace as bytes ([#62](https://github.com/feds01/bracketlint/pull/62))
- **ast:** look up keywords with a match rather than phf ([#60](https://github.com/feds01/bracketlint/pull/60))
- **ast:** store each node's range in the node ([#66](https://github.com/feds01/bracketlint/pull/66))
- **ast:** put the largest tags behind a node ([#68](https://github.com/feds01/bracketlint/pull/68))
- **fmt:** parse each piece of HTML more cheaply ([#70](https://github.com/feds01/bracketlint/pull/70))
- **cli:** write each diff and report to stderr in one write ([#71](https://github.com/feds01/bracketlint/pull/71))
- **reporting:** give each snippet only the lines that it shows ([#72](https://github.com/feds01/bracketlint/pull/72))
- **cli:** use mimalloc as the global allocator ([#69](https://github.com/feds01/bracketlint/pull/69))
- **ast:** compare names with keywords as integers ([#73](https://github.com/feds01/bracketlint/pull/73))
- **lexer:** look up whether a byte continues a name in a table ([#74](https://github.com/feds01/bracketlint/pull/74))
- **ast:** find keywords with a perfect hash ([#81](https://github.com/feds01/bracketlint/pull/81))
- **ast:** allocate the tree in an arena ([#78](https://github.com/feds01/bracketlint/pull/78))

### Documentation

- add a contributing guide, and write spec links as reference links ([#67](https://github.com/feds01/bracketlint/pull/67))
- **ast:** link each dialect's spec on the AST definitions ([#126](https://github.com/feds01/bracketlint/pull/126))

## [0.1.0] - 2026-09-14

### Fixes

- **auto-assign:** pass --repo to gh pr edit ([#8](https://github.com/feds01/bracketlint/pull/8))

