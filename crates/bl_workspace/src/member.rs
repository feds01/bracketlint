//! Definitions for [Member] and all related functionality. A [Member] is part
//! of a workspace and represents all of the associated information with a
//! single file that is being processed by the linting tool.

use std::path::PathBuf;

use bl_ast::{Dialect, LineRanges, SourceId, SpannedSource, TempSourceMap};
use derive_more::Constructor;

/// A [Member] is a file that is part of a workspace. It contains the
#[derive(Clone, Debug, Constructor)]
pub struct MemberSourceMetadata {
    line_map: LineRanges,
}

#[derive(Clone, Debug)]
pub struct Member {
    /// The fully canonicalised path of the member.
    pub path: PathBuf,

    /// The raw file contents of the member.
    pub contents: String,

    /// The template dialect that the member is written in.
    pub dialect: Dialect,

    /// Metadata about the source itself.
    metadata: MemberSourceMetadata,
}

impl Member {
    /// Create a new [Member] with the given contents.
    pub fn new(
        path: PathBuf,
        contents: String,
        dialect: Dialect,
        metadata: MemberSourceMetadata,
    ) -> Self {
        Member { path, contents, dialect, metadata }
    }

    pub fn spanned(&self) -> SpannedSource<'_> {
        SpannedSource::new(&self.contents, &self.path, &self.metadata.line_map)
    }

    pub fn contents(&self) -> &str {
        &self.contents
    }

    pub fn with_id(&self, id: SourceId) -> MemberWithId<'_> {
        MemberWithId(id, self)
    }
}

pub struct MemberWithId<'a>(SourceId, &'a Member);

impl From<MemberWithId<'_>> for TempSourceMap {
    fn from(entry: MemberWithId) -> Self {
        let MemberWithId(id, member) = entry;
        let mut map = TempSourceMap::new();
        map.add(id, member.path.clone(), member.contents.clone());
        map
    }
}
