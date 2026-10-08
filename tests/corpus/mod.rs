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
//!
//! With `CORPUS_COMPARE_WITH` set to a directory of baselines, such as those
//! of `main`, the test also writes how the templates do compared with them to
//! `target/corpus/comparison.md` (see [compare]). CI comments on each pull
//! request with it, and `just corpus-compare` shows it.
#![cfg(test)]

mod check;
mod compare;
mod report;
mod source;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use check::CheckedSource;
use source::Source;

use crate::assert_snapshot;

/// The directory of the corpus, `tests/corpus`.
fn corpus_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/corpus"))
}

/// The directory that the corpus downloads its sources into, and writes the
/// comparison to, `target/corpus`.
fn target_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/corpus")
}

#[test]
fn corpus() {
    let sources = Source::read_all();
    let results: Vec<_> = sources.iter().map(CheckedSource::new).collect();
    println!("{}", report::render(&results));

    if let Some(base) = env::var_os("CORPUS_COMPARE_WITH") {
        let comparison = compare::render(&results, Path::new(&base));
        fs::create_dir_all(target_dir()).unwrap();
        fs::write(target_dir().join("comparison.md"), comparison).unwrap();
    }

    for checked in &results {
        let path = corpus_dir().join("baselines").join(format!("{}.txt", checked.source.name));
        assert_snapshot(&path, &checked.baseline()).unwrap();
    }
}
