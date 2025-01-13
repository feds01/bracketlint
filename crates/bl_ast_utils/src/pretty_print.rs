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
    type Error = Infallible;

    type IfClauseRet = TreeNode;
    fn visit_if_clause(
        &self,
        node: ast::AstNodeRef<ast::IfClause>,
    ) -> Result<Self::IfClauseRet, Self::Error> {
        let walk::IfClause { condition, if_body } = walk::walk_if_clause(self, node)?;

        Ok(TreeNode::branch("if_clause", vec![
            TreeNode::branch("condition", vec![condition]),
            TreeNode::branch("if_body", vec![if_body]),
        ]))
    }

    type DocumentRet = TreeNode;
    fn visit_document(
        &self,
        node: ast::AstNodeRef<ast::Document>,
    ) -> Result<Self::DocumentRet, Self::Error> {
        let walk::Document { children } = walk::walk_document(self, node)?;
        Ok(TreeNode::branch("document", children))
    }

    type ArgRet = TreeNode;

    fn visit_arg(&self, node: ast::AstNodeRef<ast::Arg>) -> Result<Self::ArgRet, Self::Error> {
        let walk::Arg { name, value } = walk::walk_arg(self, node)?;

        let mut nodes = vec![];

        if let Some(name) = name {
            nodes.push(name);
        }

        if let Some(value) = value {
            nodes.push(value);
        }

        Ok(TreeNode::branch("arg", nodes))
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

    type RawRet = TreeNode;

    fn visit_raw(&self, node: ast::AstNodeRef<ast::Raw>) -> Result<Self::RawRet, Self::Error> {
        let walk::Raw { block_body } = walk::walk_raw(self, node)?;

        Ok(TreeNode::branch("raw", vec![block_body]))
    }

    type VarRet = TreeNode;

    fn visit_var(&self, node: ast::AstNodeRef<ast::Var>) -> Result<Self::VarRet, Self::Error> {
        let walk::Var { name } = walk::walk_var(self, node)?;

        Ok(TreeNode::branch("var", vec![name]))
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

    type BinExprRet = TreeNode;

    fn visit_bin_expr(
        &self,
        node: ast::AstNodeRef<ast::BinExpr>,
    ) -> Result<Self::BinExprRet, Self::Error> {
        let walk::BinExpr { lhs, op, rhs } = walk::walk_bin_expr(self, node)?;

        Ok(TreeNode::branch("bin_expr", vec![
            TreeNode::branch("lhs", vec![lhs]),
            op,
            TreeNode::branch("rhs", vec![rhs]),
        ]))
    }

    type ExprRet = TreeNode;

    fn visit_expr(&self, node: ast::AstNodeRef<ast::Expr>) -> Result<Self::ExprRet, Self::Error> {
        walk::walk_expr_same_children(self, node)
    }

    type ImportRet = TreeNode;

    fn visit_import(
        &self,
        node: ast::AstNodeRef<ast::Import>,
    ) -> Result<Self::ImportRet, Self::Error> {
        let walk::Import { template, names } = walk::walk_import(self, node)?;

        Ok(TreeNode::branch("import", vec![
            TreeNode::branch("template", vec![template]),
            TreeNode::branch("names", names),
        ]))
    }

    type FilteredExprRet = TreeNode;

    fn visit_filtered_expr(
        &self,
        node: ast::AstNodeRef<ast::FilteredExpr>,
    ) -> Result<Self::FilteredExprRet, Self::Error> {
        let walk::FilteredExpr { subject, filter } = walk::walk_filtered_expr(self, node)?;

        Ok(TreeNode::branch("filtered_expr", vec![
            TreeNode::branch("subject", vec![subject]),
            filter,
        ]))
    }

    type CallExprRet = TreeNode;

    fn visit_call_expr(
        &self,
        node: ast::AstNodeRef<ast::CallExpr>,
    ) -> Result<Self::CallExprRet, Self::Error> {
        let walk::CallExpr { subject, args } = walk::walk_call_expr(self, node)?;

        Ok(TreeNode::branch("call_expr", vec![
            TreeNode::branch("subject", vec![subject]),
            TreeNode::branch("args", args),
        ]))
    }

    type SuperRet = TreeNode;

    fn visit_super(&self, _: ast::AstNodeRef<ast::Super>) -> Result<Self::SuperRet, Self::Error> {
        Ok(TreeNode::leaf("super"))
    }

    type FloatLitRet = TreeNode;

    fn visit_float_lit(
        &self,
        node: ast::AstNodeRef<ast::FloatLit>,
    ) -> Result<Self::FloatLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("float_lit", self.source.hunk(node.span().range), "")))
    }

    type BodyRet = TreeNode;

    fn visit_body(&self, node: ast::AstNodeRef<ast::Body>) -> Result<Self::BodyRet, Self::Error> {
        let walk::Body { contents } = walk::walk_body(self, node)?;

        Ok(TreeNode::branch("body", contents))
    }

    type ExtendsRet = TreeNode;

    fn visit_extends(
        &self,
        node: ast::AstNodeRef<ast::Extends>,
    ) -> Result<Self::ExtendsRet, Self::Error> {
        let walk::Extends { template } = walk::walk_extends(self, node)?;
        Ok(TreeNode::branch("extends", vec![template]))
    }

    type LitExprRet = TreeNode;

    fn visit_lit_expr(
        &self,
        node: ast::AstNodeRef<ast::LitExpr>,
    ) -> Result<Self::LitExprRet, Self::Error> {
        let walk::LitExpr { lit } = walk::walk_lit_expr(self, node)?;

        Ok(TreeNode::branch("lit_expr", vec![lit]))
    }

    type BreakRet = TreeNode;

    fn visit_break(&self, _: ast::AstNodeRef<ast::Break>) -> Result<Self::BreakRet, Self::Error> {
        Ok(TreeNode::leaf("break"))
    }

    type IncludeRet = TreeNode;

    fn visit_include(
        &self,
        node: ast::AstNodeRef<ast::Include>,
    ) -> Result<Self::IncludeRet, Self::Error> {
        let walk::Include { template, context } = walk::walk_include(self, node)?;

        Ok(TreeNode::branch("include", vec![template, TreeNode::branch("context", context)]))
    }

    type IntLitRet = TreeNode;

    fn visit_int_lit(
        &self,
        node: ast::AstNodeRef<ast::IntLit>,
    ) -> Result<Self::IntLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("int_lit", self.source.hunk(node.span().range), "")))
    }

    type UnaryExprRet = TreeNode;

    fn visit_unary_expr(
        &self,
        node: ast::AstNodeRef<ast::UnaryExpr>,
    ) -> Result<Self::UnaryExprRet, Self::Error> {
        let walk::UnaryExpr { op, expr } = walk::walk_unary_expr(self, node)?;

        Ok(TreeNode::branch("unary_expr", vec![op, expr]))
    }

    type NameRet = TreeNode;

    fn visit_name(&self, node: ast::AstNodeRef<ast::Name>) -> Result<Self::NameRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("name", self.source.hunk(node.span().range), "\"")))
    }

    type StrLitRet = TreeNode;

    fn visit_str_lit(
        &self,
        node: ast::AstNodeRef<ast::StrLit>,
    ) -> Result<Self::StrLitRet, Self::Error> {
        Ok(TreeNode::leaf(labelled("str_lit", self.source.hunk(node.span().range), "")))
    }

    type ArrayExprRet = TreeNode;

    fn visit_array_expr(
        &self,
        node: ast::AstNodeRef<ast::ArrayExpr>,
    ) -> Result<Self::ArrayExprRet, Self::Error> {
        let walk::ArrayExpr { children } = walk::walk_array_expr(self, node)?;

        Ok(TreeNode::branch("array_expr", children))
    }

    type UnaryOpRet = TreeNode;

    fn visit_unary_op(
        &self,
        node: ast::AstNodeRef<ast::UnaryOp>,
    ) -> Result<Self::UnaryOpRet, Self::Error> {
        Ok(TreeNode::leaf(format!("unary operator `{}`", node.body())))
    }

    type VarExprRet = TreeNode;

    fn visit_var_expr(
        &self,
        node: ast::AstNodeRef<ast::VarExpr>,
    ) -> Result<Self::VarExprRet, Self::Error> {
        let walk::VarExpr { name } = walk::walk_var_expr(self, node)?;

        Ok(TreeNode::branch("var_expr", vec![name]))
    }

    type BinOpRet = TreeNode;

    fn visit_bin_op(
        &self,
        node: ast::AstNodeRef<ast::BinOp>,
    ) -> Result<Self::BinOpRet, Self::Error> {
        Ok(TreeNode::leaf(format!("operator `{}`", node.body())))
    }

    type IfRet = TreeNode;

    fn visit_if(&self, node: ast::AstNodeRef<ast::If>) -> Result<Self::IfRet, Self::Error> {
        let walk::If { clauses, otherwise } = walk::walk_if(self, node)?;

        let mut children = vec![TreeNode::branch("clauses", clauses)];

        if let Some(otherwise) = otherwise {
            children.push(TreeNode::branch("otherwise", vec![otherwise]));
        }

        Ok(TreeNode::branch("if", children))
    }

    type ForRet = TreeNode;

    fn visit_for(&self, node: ast::AstNodeRef<ast::For>) -> Result<Self::ForRet, Self::Error> {
        let walk::For { target, iterator, guard, loop_body, loop_empty, reverse_modifier } =
            walk::walk_for(self, node)?;

        let mut children = vec![
            TreeNode::branch("target", vec![target]),
            TreeNode::branch("iterator", vec![iterator]),
            TreeNode::branch("loop_body", vec![loop_body]),
        ];

        if let Some(loop_empty) = loop_empty {
            children.push(TreeNode::branch("loop_empty", vec![loop_empty]));
        }

        if reverse_modifier.is_some() {
            children.push(TreeNode::leaf("reversed"));
        }

        if let Some(guard) = guard {
            children.push(TreeNode::branch("guard", vec![guard]));
        }

        Ok(TreeNode::branch("for", children))
    }

    type ContinueRet = TreeNode;

    fn visit_continue(
        &self,
        _: ast::AstNodeRef<ast::Continue>,
    ) -> Result<Self::ContinueRet, Self::Error> {
        Ok(TreeNode::leaf("continue"))
    }

    type StatementRet = TreeNode;

    fn visit_statement(
        &self,
        node: ast::AstNodeRef<ast::Statement>,
    ) -> Result<Self::StatementRet, Self::Error> {
        walk::walk_statement_same_children(self, node)
    }

    type InlineRet = TreeNode;

    fn visit_inline(
        &self,
        node: ast::AstNodeRef<ast::Inline>,
    ) -> Result<Self::InlineRet, Self::Error> {
        let walk::Inline { expr } = walk::walk_inline(self, node)?;
        Ok(TreeNode::branch("inline", vec![expr]))
    }

    type AccessExprRet = TreeNode;

    fn visit_access_expr(
        &self,
        node: ast::AstNodeRef<ast::AccessExpr>,
    ) -> Result<Self::AccessExprRet, Self::Error> {
        let walk::AccessExpr { subject, field } = walk::walk_access_expr(self, node)?;
        Ok(TreeNode::branch("access_expr", vec![
            TreeNode::branch("subject", vec![subject]),
            TreeNode::branch("field", vec![field]),
        ]))
    }

    type IndexExprRet = TreeNode;

    fn visit_index_expr(
        &self,
        node: ast::AstNodeRef<ast::IndexExpr>,
    ) -> Result<Self::IndexExprRet, Self::Error> {
        let walk::IndexExpr { subject, index } = walk::walk_index_expr(self, node)?;
        Ok(TreeNode::branch("index_expr", vec![
            TreeNode::branch("subject", vec![subject]),
            TreeNode::branch("index", vec![index]),
        ]))
    }

    type FilterRet = TreeNode;

    fn visit_filter(
        &self,
        node: ast::AstNodeRef<ast::Filter>,
    ) -> Result<Self::FilterRet, Self::Error> {
        let walk::Filter { name, args } = walk::walk_filter(self, node)?;
        Ok(TreeNode::branch("filter", vec![name, TreeNode::branch("args", args)]))
    }

    type ForTargetRet = TreeNode;

    fn visit_for_target(
        &self,
        node: ast::AstNodeRef<ast::ForTarget>,
    ) -> Result<Self::ForTargetRet, Self::Error> {
        let walk::ForTarget { items } = walk::walk_for_target(self, node)?;
        let length = items.len();

        // For 1, just get the item, otherwise create a branch.
        Ok(if length == 1 {
            items.into_iter().next().unwrap()
        } else {
            TreeNode::branch("for_target", items)
        })
    }

}
