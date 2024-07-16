//! Contains all of AST definitions for HTML templates.

mod ast;
mod location;

pub use ast::*;
pub use location::{ByteRange, Span};
