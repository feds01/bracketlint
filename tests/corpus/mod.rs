//! Checks bracketlint against real-world templates in each dialect.
//!
//! The templates come from the sources in `sources.toml` (see [source]), and
//! each is parsed and formatted as `bracketlint check` and `bracketlint fmt`
//! would (see [check]).
//!
//! `baselines/` records how every template of each source does, and
//! `report.md` sums it up (see [report]). Like the snapshots of the UI tests,
//! the test fails if either differs from how the templates do now, and `just
//! update-snapshots` records them again (see [snapshot]), so that support for
//! each dialect only grows.
#![cfg(test)]

mod check;
mod report;
mod snapshot;
mod source;

use std::path::{Path, PathBuf};

use check::CheckedSource;
use source::Source;

use crate::REGENERATE_OUTPUT;

/// The directory of the corpus, `tests/corpus`.
fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

#[test]
fn corpus() {
    let sources = Source::read_all();
    let results: Vec<_> = sources.iter().map(CheckedSource::new).collect();

    let report = report::render(&results);
    println!("{report}");

    if *REGENERATE_OUTPUT {
        snapshot::record(&results, &report);
        return;
    }

    let changes = snapshot::compare(&results, &report);
    assert!(
        changes.is_empty(),
        "tests/corpus is out of date, record it with `just update-snapshots`:\n{}",
        changes.join("\n")
    );
}
