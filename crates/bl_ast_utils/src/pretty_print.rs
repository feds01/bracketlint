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

    type DocumentRet = TreeNode;
    fn visit_document(
        &self,
        node: ast::AstNodeRef<ast::Document>,
    ) -> Result<Self::DocumentRet, Self::Error> {
        let walk::Document { children } = walk::walk_document(self, node)?;
        Ok(TreeNode::branch("document", children))
    }

    type CommentRet = TreeNode;

    fn visit_comment(
        &self,
        _: ast::AstNodeRef<ast::Comment>,
    ) -> Result<Self::CommentRet, Self::Error> {
        Ok(TreeNode::leaf("comment"))
    }


    type TextRet = TreeNode;

    fn visit_text(&self, _: ast::AstNodeRef<ast::Text>) -> Result<Self::TextRet, Self::Error> {
        Ok(TreeNode::leaf("text"))
    }
    type BoolLitRet = TreeNode;

    fn visit_bool_lit(
        &self,
        node: ast::AstNodeRef<ast::BoolLit>,
    ) -> Result<Self::BoolLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("bool_lit", self.source.hunk(node.span().range), "")))
    }


    type LitRet = TreeNode;

    fn visit_lit(&self, node: ast::AstNodeRef<ast::Lit>) -> Result<Self::LitRet, Self::Error> {
        walk::walk_lit_same_children(self, node)
    }


    type FloatLitRet = TreeNode;

    fn visit_float_lit(
        &self,
        node: ast::AstNodeRef<ast::FloatLit>,
    ) -> Result<Self::FloatLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("float_lit", self.source.hunk(node.span().range), "")))
    }


    type LitExprRet = TreeNode;

    fn visit_lit_expr(
        &self,
        node: ast::AstNodeRef<ast::LitExpr>,
    ) -> Result<Self::LitExprRet, Self::Error> {
        let walk::LitExpr { lit } = walk::walk_lit_expr(self, node)?;

        Ok(TreeNode::branch("lit_expr", vec![lit]))
    }

    type IntLitRet = TreeNode;

    fn visit_int_lit(
        &self,
        node: ast::AstNodeRef<ast::IntLit>,
    ) -> Result<Self::IntLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("int_lit", self.source.hunk(node.span().range), "")))
    }


    type StrLitRet = TreeNode;

    fn visit_str_lit(
        &self,
        node: ast::AstNodeRef<ast::StrLit>,
    ) -> Result<Self::StrLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("str_lit", self.source.hunk(node.span().range), "")))
    }
}
