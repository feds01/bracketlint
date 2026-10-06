# UI test cases

Each `.html` file here is a test case, and the `.stdout`/`.stderr` files next
to it are snapshots of its output, re-generated with `just update-snapshots`.
The first line of a case configures the run, e.g. `// run=fail, stage=check`
(the keys are parsed in `tests/testing-internal/src/metadata.rs`). It isn't
part of the template: the case runs without it and the blank line after it,
so line numbers in the snapshots count from the first line of the template.

## `unsupported/` directories

An `unsupported/` directory holds **valid** templates that bracketlint doesn't
handle correctly yet. Their snapshots record today's wrong output: a parse
error (`run=fail`), the wrong tree, or formatted output that changes, drops or
misplaces part of the template. Any change in behaviour then shows up as a
failing test. Each case covers a single feature, so that it can move as soon
as that feature works.

Cases outside `unsupported/` have the right output, apart from formatter
layout quirks that every dialect shares, such as the indentation after nested
blocks, which the other `fmt/` cases already record.

When you add support for a feature, move its case out of `unsupported/`
together with its `.stdout`/`.stderr` files, set `run=pass` if it was
`run=fail`, re-generate the snapshots, and check that the new output is
correct.
