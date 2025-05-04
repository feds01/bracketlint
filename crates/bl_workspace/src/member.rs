//! Definitions for [Member] and all related functionality. A [Member] is part
//! of a workspace and represents all of the associated information with a
//! single file that is being processed by the linting tool.

use std::path::PathBuf;

use bl_ast::{self as ast, LineRanges, SourceId, SpannedSource, TempSourceMap};
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

    /// Metadata about the source itself.
    metadata: MemberSourceMetadata,

    /// The parsed document of the member.
    pub document: Option<ast::AstNode<ast::Document>>,
}

impl Member {
    /// Create a new [Member] with the given contents.
    pub fn new(
        path: PathBuf,
        contents: String,
        document: Option<ast::AstNode<ast::Document>>,
        metadata: MemberSourceMetadata,
    ) -> Self {
        Member { path, contents, document, metadata }
    }

    pub fn spanned(&self) -> SpannedSource<'_> {
        SpannedSource::new(&self.contents, &self.path, &self.metadata.line_map)
    }

    pub fn document(&self) -> Option<&ast::AstNode<ast::Document>> {
        self.document.as_ref()
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
