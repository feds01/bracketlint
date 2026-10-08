//! Checks bracketlint against real-world templates in each dialect.
//!
//! The templates come from the sources in `sources.toml` (see [source]), and
//! each is parsed and formatted as `bracketlint check` and `bracketlint fmt`
//! would (see [check]).
//!
//! `baselines/` records how every template of each source does. Like the
//! snapshots of the UI tests, the test fails if they differ from how the
//! templates do now, and `just update-snapshots` records them again, so that
//! support for each dialect only grows. The test prints a report of how each
//! dialect and source does (see [report]), which `just corpus` shows.
#![cfg(test)]

mod check;
mod report;
mod source;

use std::path::Path;

use check::CheckedSource;
use source::Source;

use crate::assert_snapshot;

/// The directory of the corpus, `tests/corpus`.
fn corpus_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/corpus"))
}

#[test]
fn corpus() {
    let sources = Source::read_all();
    let results: Vec<_> = sources.iter().map(CheckedSource::new).collect();
    println!("{}", report::render(&results));

    for checked in &results {
        let path = corpus_dir().join("baselines").join(format!("{}.txt", checked.source.name));
        assert_snapshot(&path, &checked.baseline()).unwrap();
    }
}
