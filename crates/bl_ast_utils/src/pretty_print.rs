//! AST visualisation utilities.
#![allow(dead_code, unused_variables)]
use std::convert::Infallible;

use bl_ast::{self as ast, AstVisitor, SpannedSource, walk};
use bl_utils::tree_writing::TreeNode;
use derive_more::Constructor;

/// Struct implementing [crate::visitor::AstVisitor], for the purpose of
/// transforming the AST tree into a [TreeNode] tree, for visualisation
/// purposes.
#[derive(Constructor)]
pub struct AstTreePrinter<'s> {
    /// The source of the AST.
    source: SpannedSource<'s>,
}

/// Easy way to format a [TreeNode] label with a main label as well as short
/// contents, and a quoting string.
fn labelled(label: impl ToString, contents: impl ToString, quote_str: &str) -> String {
    format!("{} {quote_str}{}{quote_str}", label.to_string(), contents.to_string())
}

impl AstVisitor for AstTreePrinter<'_> {
}
