//! Defines all of the commands that are available to the user.

pub mod check;
pub mod fmt;
pub mod version;

use std::{
    fmt::{self as std_fmt, Display, Formatter},
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
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

/// Map `f` over `items` on `threads` threads, and return what it returns for
/// each item, in the order of `items`. Each thread takes the next item that no
/// thread has taken yet, so that a large one doesn't hold up the rest, and
/// keeps a state for `f` that `init` makes, e.g. an arena to parse into. With
/// one thread, the items are mapped on this one.
pub(crate) fn map_in_parallel<T: Sync, S, R: Send>(
    items: &[T],
    threads: usize,
    init: impl Fn() -> S + Sync,
    f: impl Fn(&mut S, &T) -> R + Sync,
) -> Vec<R> {
    if threads <= 1 || items.len() <= 1 {
        let mut state = init();
        return items.iter().map(|item| f(&mut state, item)).collect();
    }

    let next = AtomicUsize::new(0);
    let mut mapped: Vec<(usize, R)> = thread::scope(|scope| {
        let workers: Vec<_> = (0..threads.min(items.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut state = init();
                    let mut mapped = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break mapped;
                        };
                        mapped.push((index, f(&mut state, item)));
                    }
                })
            })
            .collect();

        workers
            .into_iter()
            .flat_map(|worker| {
                worker.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });

    mapped.sort_unstable_by_key(|&(index, _)| index);
    mapped.into_iter().map(|(_, value)| value).collect()
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

#[cfg(test)]
mod tests {
    use super::map_in_parallel;

    #[test]
    fn mapped_in_parallel_in_the_order_of_the_items() {
        // The items take different times to map, so that the threads finish
        // them out of order.
        let items: Vec<u64> = (0..200).collect();
        let spin = |item: u64| (0..item % 7 * 10_000).fold(0u64, u64::wrapping_add);

        for threads in [1, 2, 8] {
            let mapped = map_in_parallel(
                &items,
                threads,
                || (),
                |(), &item| {
                    std::hint::black_box(spin(item));
                    item * 2
                },
            );

            let expected: Vec<_> = items.iter().map(|item| item * 2).collect();
            assert_eq!(mapped, expected, "with {threads} threads");
        }
    }
}
