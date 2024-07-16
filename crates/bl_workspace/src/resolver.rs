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
