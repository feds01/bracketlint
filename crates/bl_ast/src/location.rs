//! Definitions for data structures that are used to represent locations within
//! a given source file. Namely, we define [ByteRange], [Span], the bread and
//! butter of the location system. We also define a notion of a source, via a
//! [SourceId], which is unique to each source file that is processed by the
//! linting tool. We also define various other useful tools and useful data
//! structures that are used to represent locations within a source file.

use std::{cmp, fmt, ops::Range, path::PathBuf};

use derive_more::Constructor;
use index_vec::Idx;
use schemars::{self, JsonSchema};
use serde::{self, Serialize};

use crate::{HasSource, source::LineRanges};

pub static SOURCE_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[derive(
    Debug, Clone, Copy, Constructor, PartialEq, Eq, Hash, JsonSchema, Serialize, Ord, PartialOrd,
)]
pub struct SourceId(u32);

impl Default for SourceId {
    fn default() -> Self {
        SourceId(SOURCE_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
    }
}

impl Idx for SourceId {
    fn from_usize(idx: usize) -> Self {
        SourceId(idx as u32)
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

/// [ByteRange] represents a location of a range of tokens within the source.
/// The range itself is considered to be inclusive, so ranges such as `0:0`
/// would include the first byte of the source, and ranges like `0:1` would
/// include the first two bytes of the source.
#[derive(Debug, Eq, Hash, Clone, Copy, PartialEq, JsonSchema, Serialize)]
pub struct ByteRange(u32, u32);

impl ByteRange {
    /// Create a [ByteRange] by providing a start and end byte position.
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(end >= start, "invalid span. start > end. start={} end={}", start, end);
        ByteRange(start as u32, end as u32)
    }

    /// Create a single byte [ByteRange] by providing a start position.
    pub fn singleton(start: usize) -> Self {
        ByteRange(start as u32, start as u32)
    }

    /// This function is used to join a [ByteRange] to another [ByteRange].
    /// The assumption is made that the left hand-side [ByteRange] ends before
    /// the start of the right hand side [ByteRange]. If that is the case, then
    /// a new location is created with start position of the `self`, and the
    /// end position of the `other`. If that is not the case, the `self`
    /// span is returned.
    ///
    /// In essence, if this was the source stream:
    /// ```text
    /// --------------------------------------------------------------
    ///  ( <- self.start  self.end -> )   ( <- other.start other.end -> )
    /// --------------------------------------------------------------
    /// ```
    ///
    /// Then the two locations are joined into one, otherwise the lhs is
    /// returned.
    #[must_use]
    pub fn join(&self, other: Self) -> Self {
        if self.end() <= other.start() {
            return ByteRange::new(self.start(), other.end());
        }

        *self
    }

    /// Get the start of the [ByteRange].
    pub fn start(&self) -> usize {
        self.0.try_into().unwrap()
    }

    /// Get the end of the [ByteRange].
    pub fn end(&self) -> usize {
        self.1.try_into().unwrap()
    }

    /// Compute the actual size of the [ByteRange] by subtracting the end
    /// from start.
    pub fn len(&self) -> usize {
        (self.end() + 1) - self.start()
    }

    /// Check if the [ByteRange] is empty.
    pub fn is_empty(&self) -> bool {
        self.start() == self.end()
    }

    /// Convert the [ByteRange] into a [Span].
    pub fn into_span(self, source_id: SourceId) -> Span {
        Span::new(self, source_id)
    }
}

impl From<ByteRange> for Range<usize> {
    fn from(value: ByteRange) -> Self {
        value.start()..value.end() + 1
    }
}

impl Default for ByteRange {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl fmt::Display for ByteRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}

/// A [Span] describes the location of something that is relative to
/// a module that is within the workspace and that has an associated
/// [ByteRange].
///
/// [Span]s are only used when printing reports within the
/// `bl_reporting` crate. Ideally, data structures that need to store
/// locations of various items should use [ByteRange] and then convert into
/// [Span]s.
#[derive(Debug, Clone, Copy, Constructor, PartialEq, Eq, Hash, JsonSchema, Serialize)]
pub struct Span {
    /// The associated [ByteRange] with the [Span].
    pub range: ByteRange,

    /// The id of the source that the span is referencing.
    pub id: SourceId,
}

impl Span {
    /// Create a null-[Span], setting the range to be 0-0, and
    /// pointing to the prelude module.
    pub fn null() -> Self {
        Self::new(ByteRange::default(), SourceId::default())
    }

    /// Join the span of a [Span] with another [Span].
    ///
    /// *Note*: the `id` of both [Span]s must be the same.
    pub fn join(self, other: Self) -> Self {
        debug_assert!(self.id == other.id);

        Self { id: self.id, range: self.range.join(other.range) }
    }

    /// Get the length of the [Span].
    pub fn len(&self) -> usize {
        self.range.len()
    }

    /// Check if the [Span] is empty.
    pub fn is_empty(&self) -> bool {
        self.range.is_empty()
    }
}

/// A [SpannedSource] is a wrapper around the contents of a source file that
/// is stored in [SourceMap]. It features useful methods for extracting
/// and reading sections of the source by using [Span] or [ByteRange]s.
#[derive(Clone, Copy, Debug)]
pub struct SpannedSource<'s> {
    pub source: &'s str,
    pub path: &'s PathBuf,
    pub line_ranges: &'s LineRanges,
}

impl<'s> SpannedSource<'s> {
    /// Create a [SpannedSource] from a [String].
    pub fn new(source: &'s str, path: &'s PathBuf, line_ranges: &'s LineRanges) -> Self {
        Self { source, path, line_ranges }
    }

    /// Get a hunk of the source by the specified [ByteRange].
    pub fn hunk(&self, range: ByteRange) -> &'s str {
        // clamp the end to the `length` of the contents
        let end = cmp::min(self.source.len(), range.end() + 1);
        &self.source[range.start()..end]
    }

    /// Get the length of the source.
    pub fn len(&self) -> usize {
        self.source.len()
    }

    /// Check if the source is empty.
    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    /// Get the line number of a particular byte position.
    pub fn line_number(&self, byte: usize) -> usize {
        self.line_ranges.line_number(byte)
    }

    /// Get the byte position of the given positions line number with
    /// none-whitespace content. An example of this would be:
    /// ```ignore
    /// // 1:  let x = 1;     
    ///            ^    ^- the byte position within the line that is not a whitespace, i.e. the trimmed line range.
    ///            |
    ///            \ The byte position of the query
    /// ```
    pub fn trimmed_range(&self, line: usize) -> ByteRange {
        let range = self.line_ranges.range_for_line(line);
        let start = range.start();
        let end = range.end();

        let line = self.source[start..end].trim();
        let start = line.as_ptr() as usize - self.source.as_ptr() as usize;
        let end = start + line.len();

        ByteRange::new(start, end)
    }
}

/// This implementation is intended for debugging purposes within the
/// `bl_parser` crate.
///
/// It conveniently allows for the `SpannedSource` to act as the machinery
/// needed to render reports that are always local to a source file.
impl HasSource for SpannedSource<'_> {
    fn contents(&self, _: SourceId) -> &str {
        self.source
    }

    fn path(&self, _: SourceId) -> &str {
        self.path.to_str().unwrap()
    }
}
