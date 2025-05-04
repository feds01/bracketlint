use std::{collections::HashMap, path::PathBuf};

use bl_utils::range_map::{Range, RangeMap};
use derive_more::Constructor;
use line_span::LineSpanExt;

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

/// This struct is used a wrapper for a [RangeMap] in order to
/// implement a nice display format, amongst other things.
#[derive(Debug, Clone)]
pub struct LineRanges {
    /// The actual line map.
    map: RangeMap<usize, ()>,
}

impl LineRanges {
    /// Create a line range from a string slice.
    pub fn new_from_str(contents: &str) -> Self {
        // Pre-allocate the line ranges to a specific size by counting the number of
        // newline characters within the module source.
        let mut map = RangeMap::with_capacity(bytecount::count(contents.as_bytes(), b'\n'));

        // Now, iterate through the source and record the position of each newline
        // range, and push it into the map.
        let mut count = 0;

        for span in contents.line_spans() {
            map.append(count..=span.end(), ());
            count = span.ending();
        }

        Self { map }
    }

    /// Get the line number for a given index.
    pub fn line_number(&self, index: usize) -> usize {
        self.map.index_wrapping(index)
    }

    /// Get the line range for a given index.
    pub fn line_end(&self, index: usize) -> usize {
        self.map.key_wrapping(index).end()
    }

    /// Get the range responsible for the given line number.
    pub fn range_for_line(&self, line: usize) -> Range<usize> {
        *self.map.key(line).unwrap()
    }
}
