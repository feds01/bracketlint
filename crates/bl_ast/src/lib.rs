//! Contains all of AST definitions for HTML templates.

pub mod ast;
mod location;
mod source;

pub use ast::*;
pub use location::{ByteRange, SourceId, Span, SpannedSource};
pub use source::{HasSource, LineRanges, TempSourceMap};

pub mod visitor {
    pub use super::ast::{
        AstVisitor, AstVisitorMut, AstVisitorMutSelf, walk, walk_mut, walk_mut_self,
    };
}
