use bl_ast::{
    AstNodes, AstVisitorMutSelf, SourceId, SpannedSource, ast_visitor_mut_self_default_impl,
    walk_mut_self,
};
use bl_reporting::inline::{InlineSnippet, note_on_span};

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, FormatterContext, HasCSSParsing, HasHTMLParsing,
        HasJSParsing, LanguageType, TerminalState,
    },
    diagnostics::FmtError,
    options::FormatterOptions,
};

pub(crate) struct Formatter<'fmt, EngineAdaptor: ExternalLanguagesEngineAdaptor> {
    /// The engine that is used to format the document. This is used
    /// to format external languages, and in general to pass context
    /// around about the formatter.
    adaptor: EngineAdaptor,

    /// The buffer that is used to store the formatted document.
    buffer: String,

    /// The context of the formatter.
    ctx: FormatterContext<'fmt>,
}

impl<EngineAdaptor: ExternalLanguagesEngineAdaptor> Formatter<'_, EngineAdaptor> {
    /// Report an error to the parser diagnostics.
    ///
    /// This function is used to report an error to the parser diagnostics.
    #[inline(always)]
    pub(crate) fn _note_on_span(&self, span: bl_ast::Span, note: impl Into<String>) {
        note_on_span(InlineSnippet::new(&self.ctx.source, span), note.into());
    }
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
        options: FormatterOptions,
        id: SourceId,
        source: SpannedSource<'fmt>,
        buffer: String,
    ) -> Self {
        Self {
            adaptor,
            buffer,
            ctx: FormatterContext {
                id,
                source,
                options,
                state: TerminalState {
                    language: LanguageType::Html,
                    indent: 0,
                    continue_inline: false,
                },
            },
        }
    }

    /// Convert the formatter into the buffer.
    pub fn into_buffer(self) -> String {
        self.buffer
    }

    /// Apply an indent to the buffer.
    #[inline(always)]
    pub fn add_indent(&mut self) {
        let size = self.ctx.indent_level();

        self.buffer.push_str(" ".repeat(size as usize).as_str());
    }

    /// Push a line into the buffer.
    #[inline(always)]
    pub fn push_line(&mut self, line: &str) {
        self.add_indent();
        self.buffer.push_str(line);
        self.buffer.push('\n');
    }

    /// Push a newline into the buffer.
    pub fn end_line(&mut self) {
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
        self.ctx.increment_indent();
        f(self)?;
        self.ctx.decrement_indent();

        Ok(())
    }

    fn within_tag<F: FnOnce(&mut Self) -> Result<(), FmtError>>(
        &mut self,
        kind: TagKind,
        f: F,
    ) -> Result<(), FmtError> {
        self.push_hunk(kind.left());
        self.push_hunk(" ");

        // Run F without an indent level.
        f(self)?;

        // Add the closing tag.
        self.push_hunk(" ");
        self.push_hunk(kind.right());

        Ok(())
    }

    fn visit_list_of_formatters_with_separator<T, F>(
        &mut self,
        list: &'_ AstNodes<T>,
        fmt: &mut F,
        separator: &str,
    ) -> Result<(), FmtError>
    where
        F: FnMut(&mut Self, bl_ast::AstNodeRef<T>) -> Result<(), FmtError>,
    {
        list.iter().map(|item| item.ast_ref()).try_fold(false, |need_separator, item| {
            if need_separator {
                self.push_hunk(separator);
            }
            fmt(self, item)?;
            Ok(true)
        })?;

        Ok(())
    }

    // Extract the inline check into a separate function
    fn check_inline_statement_newline(
        &mut self,
        statement: bl_ast::AstNodeRef<bl_ast::Statement>,
        next_statement: Option<bl_ast::AstNodeRef<bl_ast::Statement>>,
    ) -> Result<(), FmtError> {
        match (statement.body(), next_statement.as_ref().map(|s| s.body())) {
            (bl_ast::Statement::Text(_), Some(bl_ast::Statement::Inline(_))) => Ok(()),
            (bl_ast::Statement::Text(_), Some(bl_ast::Statement::Tag(tag))) if tag.is_inline() => {
                Ok(())
            }

            (bl_ast::Statement::Tag(tag), _) => {
                if let bl_ast::Tag::Generic(generic) = tag {
                    let name = self.ctx.source.hunk(generic.name.ast_ref().span().range);

                    if bl_lexer::token::Keyword::try_from(name).is_ok() {
                        self.end_line();
                    }
                }

                Ok(())
            }
            (bl_ast::Statement::Inline(_), _) => {
                let span = statement.id().span().range;
                let line_end = self.ctx.source.line_ranges.line_end(span.end());

                // Check if the next line is the end of the line.
                if span.end() + 1 == line_end {
                    self.ctx.decrement_indent();
                    self.push_hunk("\n");
                } else {
                    // We need to continue the "inline" statement.
                    self.ctx.continue_inline();
                }

                Ok(())
            }
            (bl_ast::Statement::Comment(_), _) => {
                // If the statement is a comment, we need to check if
                // the next line is the end of the line.
                self.ctx.increment_indent();
                self.push_hunk("\n");

                // Additional logic can be added here that uses next_statement if needed

                Ok(())
            }
            _ => {
                self.end_line();
                Ok(())
            }
        }
    }
}

impl<E: ExternalLanguagesEngineAdaptor> AstVisitorMutSelf for Formatter<'_, E> {
    type Error = FmtError;

    ast_visitor_mut_self_default_impl!(
        hiding: Text, For, If, IfClause, Comment, Inline, Body, GenericTag, Arg, Name, AccessExpr, Lit, Filter, Block, With, Assignment,
    );

    type BlockRet = ();

    fn visit_block(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Block>,
    ) -> Result<Self::BlockRet, Self::Error> {
        let bl_ast::Block { label, block_body } = node.body();

        self.within_tag(TagKind::Block, |this| {
            this.push_hunk("block ");
            if let Some(label) = label {
                this.visit_name(label.ast_ref())?;
                this.push_hunk(" ");
            }

            Ok(())
        })?;
        self.push_line("");

        // Now visit the block body.
        self.with_block(|formatter| formatter.visit_body(block_body.ast_ref()))?;

        self.push_line("{% endblock %}");
        Ok(())
    }

    type WithRet = ();

    fn visit_with(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::With>,
    ) -> Result<Self::WithRet, Self::Error> {
        let bl_ast::With { assignments, block_body, .. } = node.body();

        self.within_tag(TagKind::Block, |this| {
            this.push_hunk("with ");

            this.visit_list_of_formatters_with_separator(
                assignments,
                &mut |this: &mut Self, assignment: bl_ast::AstNodeRef<'_, bl_ast::Assignment>| {
                    this.visit_assignment(assignment)
                },
                ", ",
            )?;

            Ok(())
        })?;
        self.end_line();

        // Now visit the block body.
        self.with_block(|formatter| formatter.visit_body(block_body.ast_ref()))?;

        self.push_line("{% endwith %}");
        Ok(())
    }

    type AssignmentRet = ();

    fn visit_assignment(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Assignment>,
    ) -> Result<Self::AssignmentRet, Self::Error> {
        let bl_ast::Assignment { name, value } = node.body();

        self.visit_name(name.ast_ref())?;
        self.push_hunk(" as ");
        self.visit_expr(value.ast_ref())?;

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
            this.push_hunk("for ");
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

            Ok(())
        })?;
        self.end_line();

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
            self.visit_if_clause(clause.ast_ref())?;
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

        self.add_indent();
        self.within_tag(TagKind::Block, |this| {
            match kind {
                bl_ast::ClauseKind::If => this.push_hunk("if "),
                bl_ast::ClauseKind::Elif => this.push_hunk("elif "),
            }
            this.visit_expr(condition.ast_ref())
        })?;
        self.end_line();

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(clause_body.ast_ref()))?;

        Ok(())
    }

    type BodyRet = ();

    fn visit_body(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Body>,
    ) -> Result<Self::BodyRet, Self::Error> {
        let bl_ast::Body { contents } = node.body();
        let statements: Vec<_> = contents.iter().collect();

        // Visit the body of the document
        for (i, item) in statements.iter().enumerate() {
            walk_mut_self::walk_statement(self, item.ast_ref())?;

            // Get optional next statement if it exists
            let next_statement =
                if i + 1 < statements.len() { Some(statements[i + 1].ast_ref()) } else { None };

            self.check_inline_statement_newline(item.ast_ref(), next_statement)?;
        }
        Ok(())
    }

    type TextRet = ();

    fn visit_text(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Text>,
    ) -> Result<Self::TextRet, Self::Error> {
        let is_inline = self.ctx.state.continue_inline;
        let text = self.ctx.source.hunk(node.span().range);

        // We need to check which language engine to use for the formatting.
        let (result, state) = match self.ctx.state.language {
            LanguageType::Html => {
                let mut engine = self.adaptor.html_engine(&self.ctx);
                let result = engine.format(text)?;
                let new_state = engine.into_state();
                (result, new_state)
            }
            LanguageType::Css => {
                let engine = self.adaptor.css_engine(&self.ctx);
                let result = engine.format(text)?;
                let new_state = engine.into_state();
                (result, new_state)
            }
            LanguageType::Js => {
                let engine = self.adaptor.js_engine(&self.ctx);
                let result = engine.format(text)?;
                let new_state = engine.into_state();
                (result, new_state)
            }
            LanguageType::Text => (text.to_string(), self.ctx.state),
        };

        let mut lines: Vec<_> = result.lines().collect();

        // Remove empty lines at the end
        while let Some(last) = lines.last() {
            if last.trim().is_empty() {
                lines.pop();
            } else {
                break;
            }
        }

        if is_inline {
            self.push_hunk(lines.join("\n").as_str());
        } else {
            for (index, line) in lines.iter().enumerate() {
                let is_last = index == lines.len() - 1;

                if is_last {
                    self.push_hunk(line);
                } else {
                    self.push_line(line);
                }
            }
        }

        self.ctx.state = state;
        Ok(())
    }

    type CommentRet = ();

    /// Visit a comment node.
    ///
    /// For comments, we just emit the content of the comment as a verbatim.
    fn visit_comment(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Comment>,
    ) -> Result<Self::CommentRet, Self::Error> {
        // @@Todo: we need to be more sophisticated about this, as we want to remember
        // where the "anchor" points of the comment are, so we can treat the whole
        // area as verbatim.
        self.within_tag(TagKind::Comment, |this| {
            let text = this.ctx.source.hunk(node.span().range);
            this.push_hunk(text);
            Ok(())
        })?;
        self.end_line();

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

    type AccessExprRet = ();

    fn visit_access_expr(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::AccessExpr>,
    ) -> Result<Self::AccessExprRet, Self::Error> {
        let bl_ast::AccessExpr { subject, field } = node.body();

        self.visit_expr(subject.ast_ref())?;
        self.push_hunk(".");
        self.visit_name(field.ast_ref())
    }

    type NameRet = ();

    fn visit_name(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Name>,
    ) -> Result<Self::NameRet, Self::Error> {
        let name = self.ctx.source.hunk(node.span().range);
        self.push_hunk(name);
        Ok(())
    }

    type FilterRet = ();

    fn visit_filter(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Filter>,
    ) -> Result<Self::FilterRet, Self::Error> {
        let bl_ast::Filter { name, args } = node.body();

        self.push_hunk("|");
        self.visit_name(name.ast_ref())?;
        self.push_hunk(":");
        self.visit_list_of_formatters_with_separator(
            args,
            &mut |this: &mut Self, arg: bl_ast::AstNodeRef<'_, bl_ast::Arg>| this.visit_arg(arg),
            " ",
        )?;

        Ok(())
    }

    type GenericTagRet = ();

    fn visit_generic_tag(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::GenericTag>,
    ) -> Result<Self::GenericTagRet, Self::Error> {
        self.within_tag(TagKind::Block, |this| {
            let bl_ast::GenericTag { name, args } = node.body();

            this.visit_name(name.ast_ref())?;
            this.push_hunk(" ");
            this.visit_list_of_formatters_with_separator(
                args,
                &mut |this: &mut Self, arg: bl_ast::AstNodeRef<'_, bl_ast::Arg>| {
                    this.visit_arg(arg)
                },
                " ",
            )?;
            Ok(())
        })
    }

    type ArgRet = ();

    fn visit_arg(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Arg>,
    ) -> Result<Self::ArgRet, Self::Error> {
        let bl_ast::Arg { name, value } = node.body();

        if let Some(name) = name {
            self.visit_name(name.ast_ref())?;
            self.push_hunk("=");
        }

        if let Some(value) = value {
            self.visit_expr(value.ast_ref())?;
        }

        Ok(())
    }

    type LitRet = ();

    fn visit_lit(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Lit>,
    ) -> Result<Self::LitRet, Self::Error> {
        match node.body() {
            bl_ast::Lit::Int(_) | bl_ast::Lit::Float(_) | bl_ast::Lit::Str(_) => {
                let lit = self.ctx.source.hunk(node.span().range);
                self.push_hunk(lit);
            }
            bl_ast::Lit::Bool(bool) => match bool.value {
                true => self.push_hunk("true"),
                false => self.push_hunk("false"),
            },
        }

        Ok(())
    }
}
