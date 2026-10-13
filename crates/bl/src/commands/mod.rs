//! Defines all of the commands that are available to the user.

pub mod check;
pub mod fmt;
pub mod version;

use std::{env, io, path::Path};

use bl_reporting::{Report, ReportKind};
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

/// The path of `path` from the current directory, as a user names it, or
/// `path` itself if it isn't in the current directory.
pub(crate) fn relative_path(path: &Path) -> &Path {
    env::current_dir().ok().and_then(|dir| path.strip_prefix(dir).ok()).unwrap_or(path)
}

/// An error for the file at `path`, which couldn't be read or written, as
/// `action` says. It fails the run, as the file isn't checked or formatted.
pub(crate) fn file_error(action: &str, path: &Path, err: &io::Error) -> Report {
    error(format!("couldn't {action} `{}`: {err}", relative_path(path).display()))
}

/// An error for a path that couldn't be searched for templates, e.g. one that
/// doesn't exist. It fails the run, as the templates in it aren't checked or
/// formatted.
pub(crate) fn walk_error(err: &ignore::Error) -> Report {
    match err {
        ignore::Error::WithDepth { err, .. } => walk_error(err),
        ignore::Error::WithPath { path, err } => {
            error(format!("couldn't read `{}`: {err}", relative_path(path).display()))
        }
        err => error(err),
    }
}

/// An error that is only its `title`.
fn error(title: impl ToString) -> Report {
    let mut report = Report::new();
    report.kind(ReportKind::Error).title(title);
    report
}
