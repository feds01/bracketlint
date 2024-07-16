//! Implementation of the logic that searches for all of the files that can be
//! "linted" by `bracketlint` in the set of the given file.s

use std::{
    cmp::Ordering,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::RwLock,
};

use anyhow::{anyhow, Result};
use bl_utils::fs;
use globset::{Candidate, GlobSet};
use ignore::{DirEntry, Error, WalkBuilder, WalkState};
use itertools::Itertools;
use log::debug;

use crate::settings::Settings;

// @@Todo: add a way to add exclusions to the search.
pub struct Resolver<'a> {
    settings: &'a Settings,
}

impl<'a> Resolver<'a> {
    pub fn new(settings: &'a Settings) -> Self {
        Resolver { settings }
    }

    /// Check whether we should respect `.gitignore` files.
    pub fn respect_gitignore(&self) -> bool {
        self.settings.respect_gitignore
    }

    /// Whether to enforce file exclusions.
    pub fn force_exclude(&self) -> bool {
        self.settings.file_resolver.force_exclude
    }

    pub fn resolve(&self, _: &Path) -> &Settings {
        self.settings
    }
}

pub fn find_files_in_paths(paths: &[PathBuf], settings: &Settings) -> Result<ResolvedFiles> {
    // Create a resolver, and then use it to aid in the search for files.
    let resolver = Resolver::new(settings);

    // Normalize every path (e.g., convert from relative to absolute).
    let mut paths: Vec<PathBuf> = paths.iter().map(fs::normalize_path).unique().collect();

    // Check if the paths themselves are excluded.
    if resolver.force_exclude() {
        paths.retain(|path| !is_file_excluded(path, &resolver));
        if paths.is_empty() {
            return Ok(vec![]);
        }
    }

    let (first_path, rest_paths) = paths
        .split_first()
        .ok_or_else(|| anyhow!("Expected at least one path to search for Python files"))?;

    // Create the `WalkBuilder`.
    let mut builder = WalkBuilder::new(first_path);
    for path in rest_paths {
        builder.add(path);
    }

    builder.hidden(false);
    builder.standard_filters(resolver.respect_gitignore());

    builder.threads(
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(12),
    );

    let walker = builder.build_parallel();

    let state = WalkFilesState::new(resolver);
    let mut visitor = FilesVisitorBuilder::new(&state);
    walker.visit(&mut visitor);

    state.finish()
}

type ResolvedFiles = Vec<Result<ResolvedFile, ignore::Error>>;

pub struct WalkFilesState<'a> {
    resolver: RwLock<Resolver<'a>>,
    merged: std::sync::Mutex<(ResolvedFiles, Result<()>)>,
}

impl<'a> WalkFilesState<'a> {
    pub fn new(resolver: Resolver<'a>) -> Self {
        WalkFilesState {
            resolver: RwLock::new(resolver),
            merged: std::sync::Mutex::new((Vec::new(), Ok(()))),
        }
    }

    fn finish(self) -> Result<ResolvedFiles> {
        let (files, error) = self.merged.into_inner().unwrap();
        error?;

        Ok(files)
    }
}

struct FilesVisitorBuilder<'s, 'config> {
    state: &'s WalkFilesState<'config>,
}

impl<'s, 'config> FilesVisitorBuilder<'s, 'config> {
    fn new(state: &'s WalkFilesState<'config>) -> Self {
        FilesVisitorBuilder { state }
    }
}

impl<'config, 's> ignore::ParallelVisitorBuilder<'s> for FilesVisitorBuilder<'s, 'config>
where
    'config: 's,
{
    fn build(&mut self) -> Box<dyn ignore::ParallelVisitor + 's> {
        Box::new(FilesVisitor { local_files: vec![], local_error: Ok(()), global: self.state })
    }
}

pub struct FilesVisitor<'s, 'config> {
    local_files: Vec<Result<ResolvedFile, ignore::Error>>,
    local_error: Result<()>,
    global: &'s WalkFilesState<'config>,
}

impl<'s, 'config> ignore::ParallelVisitor for FilesVisitor<'s, 'config> {
    fn visit(&mut self, result: std::result::Result<DirEntry, Error>) -> WalkState {
        // Respect our own exclusion behaviour.
        if let Ok(entry) = &result {
            if entry.depth() > 0 {
                let path = entry.path();
                let resolver = self.global.resolver.read().unwrap();
                let settings = resolver.resolve(path);

                if let Some(file_name) = path.file_name() {
                    let file_path = Candidate::new(path);
                    let file_basename = Candidate::new(file_name);
                    if match_candidate_exclusion(
                        &file_path,
                        &file_basename,
                        &settings.file_resolver.exclude,
                    ) {
                        return WalkState::Skip;
                    }
                } else {
                    return WalkState::Skip;
                }
            }
        }

        match result {
            Ok(entry) => {
                // Ignore directories
                let resolved = if entry.file_type().map_or(true, |ft| ft.is_dir()) {
                    None
                } else if entry.depth() == 0 {
                    // Accept all files that are passed-in directly.
                    Some(ResolvedFile::Root(entry.into_path()))
                } else {
                    // Otherwise, check if the file is included.
                    let path = entry.path();
                    let resolver = self.global.resolver.read().unwrap();
                    let settings = resolver.resolve(path);

                    if settings.file_resolver.include.is_match(path) {
                        Some(ResolvedFile::Nested(entry.into_path()))
                    } else {
                        None
                    }
                };

                if let Some(resolved) = resolved {
                    self.local_files.push(Ok(resolved));
                }
            }
            Err(err) => {
                self.local_files.push(Err(err));
            }
        }

        WalkState::Continue
    }
}

impl Drop for FilesVisitor<'_, '_> {
    fn drop(&mut self) {
        let mut merged = self.global.merged.lock().unwrap();
        let (ref mut files, ref mut error) = &mut *merged;

        if files.is_empty() {
            *files = std::mem::take(&mut self.local_files);
        } else {
            files.append(&mut self.local_files);
        }

        let local_error = std::mem::replace(&mut self.local_error, Ok(()));
        if error.is_ok() {
            *error = local_error;
        }
    }
}

pub fn match_candidate_exclusion(
    file_path: &Candidate,
    file_basename: &Candidate,
    exclusion: &GlobSet,
) -> bool {
    if exclusion.is_empty() {
        return false;
    }
    exclusion.is_match_candidate(file_path) || exclusion.is_match_candidate(file_basename)
}

pub fn is_file_excluded(path: &Path, resolver: &Resolver) -> bool {
    for path in path.ancestors() {
        let settings = resolver.resolve(path);
        if let Some(file_name) = path.file_name() {
            let file_path = Candidate::new(path);
            let file_basename = Candidate::new(file_name);
            if match_candidate_exclusion(
                &file_path,
                &file_basename,
                &settings.file_resolver.exclude,
            ) {
                debug!("Ignored path via `exclude`: {:?}", path);
                return true;
            } else if match_candidate_exclusion(
                &file_path,
                &file_basename,
                &settings.file_resolver.user_exclude,
            ) {
                return true;
            }
        } else {
            break;
        }
    }
    false
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedFile {
    /// File explicitly passed to the CLI
    Root(PathBuf),
    /// File in a sub-directory
    Nested(PathBuf),
}

impl ResolvedFile {
    pub fn into_path(self) -> PathBuf {
        match self {
            ResolvedFile::Root(path) => path,
            ResolvedFile::Nested(path) => path,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            ResolvedFile::Root(root) => root.as_path(),
            ResolvedFile::Nested(root) => root.as_path(),
        }
    }

    pub fn file_name(&self) -> &OsStr {
        let path = self.path();
        path.file_name().unwrap_or(path.as_os_str())
    }

    pub fn is_root(&self) -> bool {
        matches!(self, ResolvedFile::Root(_))
    }
}

impl PartialOrd for ResolvedFile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ResolvedFile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path().cmp(other.path())
    }
}
