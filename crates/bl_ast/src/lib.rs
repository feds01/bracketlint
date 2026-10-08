//! Contains all of AST definitions for HTML templates, and the language
//! primitives that every stage shares: the [Dialect] and its [Keyword]s.

pub mod ast;
mod dialect;
mod keywords;
mod location;
mod source;

pub use ast::*;
pub use dialect::Dialect;
pub use keywords::Keyword;
pub use location::{ByteRange, SourceId, Span, SpannedSource};
pub use source::{HasSource, LineRanges, TempSourceMap};

pub mod visitor {
    pub use super::ast::{
        AstVisitor, AstVisitorMut, AstVisitorMutSelf, walk, walk_mut, walk_mut_self,
    };
}
