//! The module contains all of the AST definitions for the templates that
//! `bracketlint` supports parsing.

use crate::location::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode<T> {
    pub kind: T,

    /// The location of the node in the source file.
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {}
