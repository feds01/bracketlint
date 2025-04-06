use bl_ast::{
    AstVisitorMutSelf, SourceId, SpannedSource, ast_visitor_mut_self_default_impl, walk_mut_self,
};

use crate::{
    FmtOptions,
    adapters::{ExternalLanguagesEngineAdaptor, FormatterContext, HasHTMLParsing},
    diagnostics::FmtError,
};

pub(crate) struct Formatter<'fmt, EngineAdaptor: ExternalLanguagesEngineAdaptor> {
    /// The engine that is used to format the document. This is used
    /// to format external languages, and in general to pass context
    /// around about the formatter.
    adaptor: EngineAdaptor,

    /// The source that is used to format the document.
    source: SpannedSource<'fmt>,

    /// The buffer that is used to store the formatted document.
    buffer: String,

    /// Any options that are used to format the document.
    options: FmtOptions,

    /// The context of the formatter.
    ctx: FormatterContext,
}

pub enum TagKind {
    Block,
    Inline,
    Comment,
}

impl TagKind {
    pub fn left(&self) -> &'static str {
        match self {
            TagKind::Block => "{%",
            TagKind::Inline => "{{",
            TagKind::Comment => "{#",
        }
    }

    pub fn right(&self) -> &'static str {
        match self {
            TagKind::Block => "%}",
            TagKind::Inline => "}}",
            TagKind::Comment => "#}",
        }
    }
}

impl<'fmt, Adaptor: ExternalLanguagesEngineAdaptor> Formatter<'fmt, Adaptor> {
    pub fn new(
        adaptor: Adaptor,
        options: FmtOptions,
        id: SourceId,
        source: SpannedSource<'fmt>,
        buffer: String,
    ) -> Self {
        Self { adaptor, buffer, source, options, ctx: FormatterContext { indent: 0, source: id } }
    }

    /// Convert the formatter into the buffer.
    pub fn into_buffer(self) -> String {
        self.buffer
    }

    /// Apply an indent to the buffer.
    #[inline(always)]
    pub fn add_indent(&mut self) {
        let size = self.ctx.indent * self.options.indent_size;

        self.buffer.push_str(" ".repeat(size as usize).as_str());
    }

    /// Push a line into the buffer.
    #[inline(always)]
    pub fn push_line(&mut self, line: &str) {
        self.add_indent();
        self.buffer.push_str(line);
        self.buffer.push('\n');
    }

    /// Push a hunk on the current line.
    #[inline(always)]
    pub fn push_hunk(&mut self, hunk: &str) {
        self.buffer.push_str(hunk);
    }

    /// Run a function with an increased indent level.
    ///
    /// Assume that the formatter function is called within a block, meaning
    /// that we format the inner parts of `F` with an increased indent
    /// level.
    pub fn with_block<F>(&mut self, f: F) -> Result<(), FmtError>
    where
        F: FnOnce(&mut Self) -> Result<(), FmtError>,
    {
        // We increment the indent level before calling the function, and decrement it
        self.ctx.indent += 1;
        f(self)?;
        self.ctx.indent -= 1;

        Ok(())
    }

    fn within_tag<F: FnOnce(&mut Self) -> Result<(), FmtError>>(
        &mut self,
        kind: TagKind,
        f: F,
    ) -> Result<(), FmtError> {
        self.add_indent();
        self.push_hunk(kind.left());

        // Run F without an indent level.
        f(self)?;

        // Add the closing tag.
        self.push_hunk(kind.right());
        Ok(())
    }
}

impl<E: ExternalLanguagesEngineAdaptor> AstVisitorMutSelf for Formatter<'_, E> {
    type Error = FmtError;
    type DocumentRet = ();

    fn visit_document(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Document>,
    ) -> Result<Self::DocumentRet, Self::Error> {
        let _ = walk_mut_self::walk_document(self, node)?;
        Ok(())
    }

    type ForRet = ();

    fn visit_for(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::For>,
    ) -> Result<Self::ForRet, Self::Error> {
        let bl_ast::For { target, iterator, guard, reverse_modifier, loop_body, loop_empty } =
            node.body();

        // @@ Proof of concept: for now, we will just push a for loop into the buffer.
        self.within_tag(TagKind::Block, |this| {
            this.push_hunk(" for ");
            this.visit_for_target(target.ast_ref())?;
            this.push_hunk(" in ");
            this.visit_expr(iterator.ast_ref())?;

            // Check if we have an if guard on the loop itself.
            if let Some(guard) = guard {
                this.push_hunk(" if ");
                this.visit_expr(guard.ast_ref())?;
            }

            // Check if there's a reverse modifier on the loop.
            if let Some(reverse_modifier) = reverse_modifier {
                this.push_hunk(" reverse");
                this.visit_name(reverse_modifier.ast_ref())?;
            }

            this.push_hunk(" ");
            Ok(())
        })?;

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(loop_body.ast_ref()))?;

        // Check if we have an empty loop body.
        if let Some(loop_empty) = loop_empty {
            self.push_line("{% empty %}");
            self.with_block(|formatter| formatter.visit_body(loop_empty.ast_ref()))?;
        }

        self.push_line("{% endfor %}");
        Ok(())
    }

    type IfRet = ();

    fn visit_if(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::If>,
    ) -> Result<Self::IfRet, Self::Error> {
        let bl_ast::If { clauses, otherwise } = node.body();

        // Walk the clauses, and format each one of them.
        for clause in clauses.iter() {
            walk_mut_self::walk_if_clause(self, clause.ast_ref())?;
        }

        if let Some(otherwise) = otherwise {
            self.push_line("{% else %}");
            self.with_block(|formatter| formatter.visit_body(otherwise.ast_ref()))?;
        }

        self.push_line("{% endif %}");

        Ok(())
    }

    type IfClauseRet = ();
    fn visit_if_clause(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::IfClause>,
    ) -> Result<Self::IfClauseRet, Self::Error> {
        let bl_ast::IfClause { kind, condition, clause_body } = node.body();

        self.within_tag(TagKind::Block, |this| {
            match kind {
                bl_ast::ClauseKind::If => this.push_hunk(" if "),
                bl_ast::ClauseKind::Elif => this.push_hunk(" elif "),
            }
            this.visit_expr(condition.ast_ref())?;
            this.push_hunk(" ");
            Ok(())
        })?;

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(clause_body.ast_ref()))?;

        Ok(())
    }

    type InlineRet = ();

    fn visit_inline(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Inline>,
    ) -> Result<Self::InlineRet, Self::Error> {
        let bl_ast::Inline { expr } = node.body();

        self.within_tag(TagKind::Inline, |this| {
            this.visit_expr(expr.ast_ref())?;
            Ok(())
        })
    }

    type VarExprRet = ();

    fn visit_var_expr(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::VarExpr>,
    ) -> Result<Self::VarExprRet, Self::Error> {
        let name = self.source.hunk(node.span().range);
        self.push_hunk(name);
        Ok(())
    }
}
