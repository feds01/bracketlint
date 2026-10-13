//! Defines all of the commands that are available to the user.

pub mod check;
pub mod fmt;
pub mod version;

use std::{
    fmt::{self as std_fmt, Display, Formatter},
    io,
    path::{Path, PathBuf},
};

use bl_reporting::{ReportBuilder, Reports};
use bl_utils::path::relative_path;
use bl_workspace::resolver::ResolvedFile;

/// Sort the `files` that the walker found by their paths, after the paths
/// that it couldn't search. It searches them in parallel, so it finds them in
/// no set order, but each run should name the files, and report the problems
/// in them, in the same order.
pub(crate) fn sort_files(files: &mut [Result<ResolvedFile, ignore::Error>]) {
    fn path(file: &Result<ResolvedFile, ignore::Error>) -> Option<&Path> {
        file.as_ref().ok().map(ResolvedFile::path)
    }

    files.sort_by(|a, b| path(a).cmp(&path(b)));
}

/// A file that couldn't be read or written, or a path that couldn't be
/// searched for templates, e.g. one that doesn't exist. It fails the run, as
/// the templates in it aren't checked or formatted.
#[derive(Debug)]
pub(crate) enum FileError {
    /// The file at `path` couldn't be read.
    Read { path: PathBuf, err: io::Error },

    /// The file at `path` couldn't be written.
    Write { path: PathBuf, err: io::Error },

    /// A path couldn't be searched.
    Walk(ignore::Error),
}

impl From<ignore::Error> for FileError {
    fn from(err: ignore::Error) -> Self {
        FileError::Walk(err)
    }
}

impl Display for FileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std_fmt::Result {
        match self {
            FileError::Read { path, err } => file_message(f, "read", path, err),
            FileError::Write { path, err } => file_message(f, "write", path, err),
            FileError::Walk(err) => walk_message(f, err),
        }
    }
}

/// The message that the file at `path` couldn't be read or written, as
/// `action` says, because of `err`.
fn file_message(
    f: &mut Formatter<'_>,
    action: &str,
    path: &Path,
    err: &dyn Display,
) -> std_fmt::Result {
    write!(f, "couldn't {action} `{}`: {err}", relative_path(path).display())
}

/// The message that a path couldn't be searched because of `err`.
fn walk_message(f: &mut Formatter<'_>, err: &ignore::Error) -> std_fmt::Result {
    match err {
        ignore::Error::WithDepth { err, .. } => walk_message(f, err),
        ignore::Error::WithPath { path, err } => file_message(f, "read", path, err),
        err => write!(f, "{err}"),
    }
}

impl From<FileError> for Reports {
    fn from(err: FileError) -> Self {
        let mut reporter = ReportBuilder::default();
        reporter.error().title(err);
        reporter.into_reports()
    }
}
