//! Contains all of AST definitions for HTML templates.

mod ast;
mod location;

pub use ast::*;
pub use location::{ByteRange, SourceId, Span, SpannedSource};

pub mod visitor {
    pub use super::ast::{
        walk, walk_mut, walk_mut_self, AstVisitor, AstVisitorMut, AstVisitorMutSelf,
    };
}
