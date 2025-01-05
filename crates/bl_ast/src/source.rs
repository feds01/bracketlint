use std::{collections::HashMap, path::PathBuf};

use derive_more::Constructor;

use crate::SourceId;

/// A trait for accessing and reading basic information about a source via a its
/// [SourceId].
///
/// This trait is used to abstract the source of a file, allowing for different
/// implementations to be used in different contexts.
pub trait HasSource {
    /// Get the contents of a particular source item.
    fn contents(&self, source: SourceId) -> &str;

    /// Get the path of a particular source item.
    fn path(&self, source: SourceId) -> &str;
}

/// A temporary (useful debugging) implementation of [HasSource] that stores the
/// source within a [HashMap] and can be used during testing and or development
/// without the need of the workspace or any centralised source map storage.
#[derive(Debug, Constructor)]
struct TempEntry {
    /// The path of the source.
    pub path: PathBuf,

    /// The contents of the source.
    pub contents: String,
}

/// A temporary (useful debugging) implementation of [HasSource] that stores the
/// source within a [HashMap] and can be used during testing and or development
/// without the need of the workspace or any centralised source map storage.
#[derive(Debug)]
pub struct TempSourceMap {
    /// The internal map of sources.
    map: HashMap<SourceId, TempEntry>,
}

impl Default for TempSourceMap {
    fn default() -> Self {
        Self::new()
    }
}

impl TempSourceMap {
    /// Create a new [TempSourceMap].
    pub fn new() -> Self {
        TempSourceMap { map: HashMap::new() }
    }

    /// Add a new source to the map.
    pub fn add(&mut self, id: SourceId, path: PathBuf, contents: String) -> &mut Self {
        self.map.insert(id, TempEntry::new(path, contents));
        self
    }
}

impl HasSource for TempSourceMap {
    fn contents(&self, source: SourceId) -> &str {
        self.map[&source].contents.as_str()
    }

    fn path(&self, source: SourceId) -> &str {
        self.map[&source].path.as_path().to_str().unwrap()
    }
}
