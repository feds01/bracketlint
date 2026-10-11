use std::{collections::HashMap, ops::Range, path::PathBuf};

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

/// The lines of a source, used to find the line that a byte is on.
#[derive(Debug, Clone)]
pub struct LineRanges {
    /// The range of each line, from its first byte to the end of its text,
    /// before its `\n` or `\r\n` line ending. The lines follow one another,
    /// so a byte is on the last line that starts at or before it, which
    /// puts a line ending on the line that it ends.
    lines: Vec<Range<usize>>,
}

impl LineRanges {
    /// Create a line range from a string slice.
    pub fn new_from_str(contents: &str) -> Self {
        // Pre-allocate the line ranges to a specific size by counting the number of
        // newline characters within the module source.
        let mut lines = Vec::with_capacity(bytecount::count(contents.as_bytes(), b'\n'));
        lines.extend(contents.line_spans().map(|span| span.range()));

        Self { lines }
    }

    /// Get the line number for a given index. An index past the end of the
    /// source is on the last line.
    pub fn line_number(&self, index: usize) -> usize {
        self.lines.partition_point(|line| line.start <= index) - 1
    }

    /// Get the end of the text of the line that the given index is on,
    /// before its line ending.
    pub fn line_end(&self, index: usize) -> usize {
        self.lines[self.line_number(index)].end
    }

    /// Get the range of the given line number, without its line ending.
    pub fn range_for_line(&self, line: usize) -> Range<usize> {
        self.lines[line].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::LineRanges;

    /// Each byte is on its line, the bytes of its line ending included, and
    /// the end of the source is on the last line.
    #[test]
    fn each_byte_is_on_its_line() {
        let sources: [(&str, &[usize]); 3] = [
            ("a\nb\nc", &[0, 0, 1, 1, 2, 2]),
            ("a\r\nb\r\nc", &[0, 0, 0, 1, 1, 1, 2, 2]),
            ("a\r\n\r\nb\r\n", &[0, 0, 0, 1, 1, 2, 2, 2, 2]),
        ];

        for (contents, expected) in sources {
            let ranges = LineRanges::new_from_str(contents);
            let lines: Vec<_> =
                (0..=contents.len()).map(|index| ranges.line_number(index)).collect();

            assert_eq!(lines, expected, "{contents:?}");
        }
    }

    /// A line ends where its text does, before its line ending, from any of
    /// its bytes, and a line number finds that line's range.
    #[test]
    fn lines_end_before_their_line_ending() {
        let contents = "a\r\nbc\nd";
        let ranges = LineRanges::new_from_str(contents);

        let ends: Vec<_> = (0..=contents.len()).map(|index| ranges.line_end(index)).collect();
        assert_eq!(ends, [1, 1, 1, 5, 5, 5, 7, 7]);

        let lines: Vec<_> = (0..3).map(|line| ranges.range_for_line(line)).collect();
        assert_eq!(lines, [0..1, 3..5, 6..7]);
    }
}
