use bl_ast::{
    AstVisitorMutSelf, ByteRange, Dialect, SourceId, SpannedSource,
    ast_visitor_mut_self_default_impl, walk_mut_self,
};
use bl_reporting::inline::{InlineSnippet, note_on_span};

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, FormatterContext, HasCSSParsing, HasHTMLParsing, HtmlPart,
        LanguageType, TerminalState,
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

    /// Where the source that the buffer ends with ends, when the buffer ends
    /// with source that is pushed exactly as it is written, with the length of
    /// the buffer then. In a `<pre>` or `<textarea>`, the source after it is
    /// pushed exactly as it is written too, from there.
    exact: Option<(usize, usize)>,
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

/// Whether a run of whitespace separates two items on the same line.
fn is_inline_space(whitespace: &str) -> bool {
    !whitespace.is_empty() && !whitespace.contains('\n')
}

pub enum TagKind {
    Block,
    Inline,
}

impl TagKind {
    pub fn left(&self) -> &'static str {
        match self {
            TagKind::Block => "{%",
            TagKind::Inline => "{{",
        }
    }

    pub fn right(&self) -> &'static str {
        match self {
            TagKind::Block => "%}",
            TagKind::Inline => "}}",
        }
    }
}

impl<'fmt, Adaptor: ExternalLanguagesEngineAdaptor> Formatter<'fmt, Adaptor> {
    pub fn new(
        adaptor: Adaptor,
        options: FormatterOptions,
        id: SourceId,
        source: SpannedSource<'fmt>,
        dialect: Dialect,
        buffer: String,
    ) -> Self {
        Self {
            adaptor,
            buffer,
            ctx: FormatterContext {
                id,
                source,
                options,
                dialect,
                state: TerminalState { language: LanguageType::Html, indent: 0 },
                open_elements: vec![],
            },
            exact: None,
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

    /// Whether the next hunk would start a new line in the buffer.
    #[inline(always)]
    fn at_line_start(&self) -> bool {
        self.buffer.is_empty() || self.buffer.ends_with('\n')
    }

    /// The whitespace in the source right before `start`.
    fn whitespace_before(&self, start: usize) -> &'fmt str {
        let before = &self.ctx.source.source[..start];
        &before[before.trim_end().len()..]
    }

    /// The whitespace in the source right after `range`.
    fn whitespace_after(&self, range: ByteRange) -> &'fmt str {
        let after = &self.ctx.source.source[range.end() + 1..];
        &after[..after.len() - after.trim_start().len()]
    }

    /// Separate an item that continues the current line from what precedes
    /// it with a single space, when the source has one. The lexer skips
    /// whitespace and the engines trim it, which would otherwise turn
    /// `{{ a }}, {{ b }}` into `{{ a }},{{ b }}` and change the output.
    fn space_from_previous(&mut self, start: usize) {
        if is_inline_space(self.whitespace_before(start))
            && !self.at_line_start()
            && !self.buffer.ends_with(char::is_whitespace)
        {
            self.push_hunk(" ");
        }
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

    /// Push the source at `range` exactly as it is written. This is used for
    /// anything the formatter can't rebuild from the tree without losing part
    /// of it, e.g. a tag that the parser doesn't know.
    fn push_source(&mut self, range: ByteRange) {
        let source = self.ctx.source.hunk(range);
        self.push_hunk(source);
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

    /// Indent a tag of a block, when it starts a line, so that the tags of the
    /// block line up. One that continues a line, e.g. the `{% endif %}` of
    /// `{% if a %}b{% endif %}`, isn't indented.
    fn indent_tag(&mut self) {
        if self.at_line_start() {
            self.add_indent();
        }
    }

    /// Start an item that starts at `start` in the source, e.g. a `{{ }}` tag:
    /// on a line of its own, at the current indent, when the source starts one
    /// with it, and otherwise after a space when the source has one.
    fn start_item(&mut self, start: usize) {
        if self.whitespace_before(start).contains('\n') && !self.at_line_start() {
            self.end_line();
        }

        if self.at_line_start() {
            self.add_indent();
        } else {
            self.space_from_previous(start);
        }
    }

    /// Start a branch of a block, e.g. the `{% else %}` of an if block, with
    /// the elements `open` at the start of the block. Each branch opens and
    /// closes its own, and the tags of the block line up with its start.
    fn start_branch(&mut self, open: &[String]) {
        self.ctx.set_open_elements(open.to_vec());
    }

    /// End a block with its end tag, at the elements `open` at its start, after
    /// which the elements `after` are open: those that its first branch leaves
    /// open, as the template would be if it took it.
    fn end_block(
        &mut self,
        end: &str,
        trim: bl_ast::TrimMarker,
        open: &[String],
        after: Vec<String>,
    ) -> Result<(), FmtError> {
        self.start_branch(open);
        self.push_tag_line(end, trim)?;
        self.ctx.set_open_elements(after);

        Ok(())
    }

    /// Push a tag whose contents `f` pushes, with the whitespace control
    /// markers of `trim` next to its delimiters.
    fn within_tag<F: FnOnce(&mut Self) -> Result<(), FmtError>>(
        &mut self,
        kind: TagKind,
        trim: bl_ast::TrimMarker,
        f: F,
    ) -> Result<(), FmtError> {
        self.push_hunk(kind.left());
        self.push_marker(trim.left);
        self.push_hunk(" ");

        // Run F without an indent level.
        f(self)?;

        // Add the closing tag.
        self.push_hunk(" ");
        self.push_marker(trim.right);
        self.push_hunk(kind.right());

        Ok(())
    }

    /// Push a line with a `{% ... %}` tag that holds `contents`, e.g. the
    /// `{% endif %}` of an if block.
    fn push_tag_line(&mut self, contents: &str, trim: bl_ast::TrimMarker) -> Result<(), FmtError> {
        self.indent_tag();
        self.within_tag(TagKind::Block, trim, |this| {
            this.push_hunk(contents);
            Ok(())
        })?;
        self.end_line();

        Ok(())
    }

    /// Push a whitespace control marker, if there is one.
    fn push_marker(&mut self, marker: Option<char>) {
        if let Some(marker) = marker {
            self.buffer.push(marker);
        }
    }

    /// Push `text`, which starts at `start` in the source, formatted in the
    /// current language, or as it is written if it doesn't parse.
    fn push_text(&mut self, start: usize, text: &str) -> Result<(), FmtError> {
        // We need to check which language engine to use for the formatting.
        let (result, state) = match self.ctx.state.language {
            LanguageType::Html => return self.push_html(start, text),
            LanguageType::Css => {
                let engine = self.adaptor.css_engine(&self.ctx);
                let result = engine.format(text)?;
                (result, engine.into_state())
            }
            // The text is only JavaScript when a tag splits a `<script>` element,
            // and a piece of a script can't be formatted on its own without
            // changing it. Biome drops the `;` of `{{ data }};`, and makes
            // `{{ f }}\n(x)`, a call, into `{{ f }}\nx;`.
            LanguageType::Js => (None, self.ctx.state),
            LanguageType::Text => (Some(text.to_string()), self.ctx.state),
        };

        self.push_piece(start, text, result);
        self.ctx.state = state;
        Ok(())
    }

    /// Push the piece of HTML `text`, which starts at `start` in the source,
    /// part by part, and open and close the elements that it does.
    fn push_html(&mut self, start: usize, text: &str) -> Result<(), FmtError> {
        let parts = self.adaptor.html_engine(&self.ctx).format(text)?;

        for part in parts {
            match part {
                HtmlPart::Html { range, formatted, opens } => {
                    // The element that the part opens may end one that is open,
                    // e.g. an `<li>` the `<li>` before it, so the part is outside
                    // of that.
                    if let Some(name) = opens.first() {
                        self.ctx.close_implied(name);
                    }

                    // The content of a `<pre>` or `<textarea>` that a tag splits
                    // is kept exactly as it is written, since its whitespace
                    // matters.
                    if self.ctx.keeps_whitespace() {
                        self.push_exact(start + range.start, &text[range]);
                    } else {
                        self.push_piece(start + range.start, &text[range], formatted);
                    }

                    self.ctx.open_elements(opens);
                }
                HtmlPart::Verbatim { range } => self.push_exact(start + range.start, &text[range]),
                HtmlPart::Close { range, name } => {
                    // The closing tag of a `<pre>` or `<textarea>` stays where it
                    // is, as the whitespace before it is its content. Any other
                    // is at the indent of the elements that are still open.
                    let tag_start = start + range.start;
                    if self.ctx.keeps_whitespace() {
                        self.push_exact(tag_start, &text[range]);
                        self.ctx.close_element(&name);
                    } else {
                        self.ctx.close_element(&name);
                        self.start_item(tag_start);
                        self.push_hunk(&text[range]);
                    }
                }
            }
        }

        Ok(())
    }

    /// Push `text`, which starts at `start` in the source, exactly as it is
    /// written, e.g. the content of a `<pre>`, but for the whitespace at its
    /// end when it ends a line, which the formatter lays out with what comes
    /// next, e.g. a `</script>`. The last line is left open, as
    /// [Self::push_lines] leaves it. In a `<pre>` or `<textarea>`, what comes
    /// next is pushed exactly too, from the whitespace before it, see
    /// [Self::push_exact_gap].
    fn push_exact(&mut self, start: usize, text: &str) {
        self.push_exact_gap(start);

        let trimmed = text.trim_end();
        let text = if text[trimmed.len()..].contains('\n') { trimmed } else { text };
        self.push_hunk(text);
        self.exact = Some((start + text.len(), self.buffer.len()));
    }

    /// Push the source before `start` exactly as it is written, in a `<pre>`
    /// or `<textarea>`: the whitespace from where the buffer ends with source
    /// pushed exactly, or else the whitespace right before `start`.
    fn push_exact_gap(&mut self, start: usize) {
        if !self.ctx.keeps_whitespace() {
            return;
        }

        let exact_end = self.exact.filter(|&(_, len)| len == self.buffer.len());
        let from = match exact_end {
            Some((end, _)) => end,
            None => start - self.whitespace_before(start).len(),
        };

        if from < start {
            self.push_hunk(&self.ctx.source.source[from..start]);
        }
    }

    /// Push `text`, which starts at `start` in the source, as `formatted`, or
    /// as it is written if that is `None`.
    fn push_piece(&mut self, start: usize, text: &str, formatted: Option<String>) {
        let start = start + (text.len() - text.trim_start().len());

        // The text starts a line when the source starts one with it.
        if !text.trim().is_empty()
            && self.whitespace_before(start).contains('\n')
            && !self.at_line_start()
        {
            self.end_line();
        }

        self.space_from_previous(start);

        match formatted {
            Some(formatted) => self.push_lines(&formatted),
            // Text that is only whitespace, e.g. the indent of a `</script>`,
            // has nothing to keep.
            None => {
                let text = text.trim();
                if !text.is_empty() && self.at_line_start() {
                    self.indent_kept_source(start, text);
                }

                self.push_hunk(text);
            }
        }
    }

    /// Push the lines of `formatted`, each at the current indent, without its
    /// empty lines at the end.
    ///
    /// The last line is left open so that a following inline tag can continue
    /// it. Any line that starts a new line in the buffer gets the current
    /// indent, the first one may instead continue a line that an inline tag
    /// left open.
    fn push_lines(&mut self, formatted: &str) {
        let mut lines: Vec<_> = formatted.lines().collect();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }

        for (index, line) in lines.into_iter().enumerate() {
            if index > 0 {
                self.end_line();
            }

            if self.at_line_start() && !line.is_empty() {
                self.add_indent();
            }

            self.push_hunk(line);
        }
    }

    /// Indent `source`, which starts at `start` and a line, and which is kept
    /// as it is written. When it spans several lines, it keeps its column, so
    /// that its other lines still line up with it. Otherwise, it gets the
    /// current indent.
    fn indent_kept_source(&mut self, start: usize, source: &str) {
        let whitespace = self.whitespace_before(start);

        match whitespace.rfind('\n') {
            Some(newline) if source.contains('\n') => self.push_hunk(&whitespace[newline + 1..]),
            _ => self.add_indent(),
        }
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
                let keyword = match tag {
                    bl_ast::Tag::Generic(generic) => {
                        let name = self.ctx.source.hunk(generic.name.ast_ref().range());
                        self.ctx.dialect.keyword(name).is_some()
                    }
                    _ => false,
                };

                // A tag that ends a line in the source ends it here too, e.g. an
                // `{% include %}` on a line of its own.
                let ends_line = self.whitespace_after(statement.range()).contains('\n');

                if (keyword || ends_line) && !self.at_line_start() {
                    self.end_line();
                }

                Ok(())
            }
            (bl_ast::Statement::Inline(_), _) => {
                let span = statement.range();
                let line_end = self.ctx.source.line_ranges.line_end(span.end());

                // Check if the next line is the end of the line, otherwise the
                // following text continues the line.
                if span.end() + 1 == line_end {
                    self.push_hunk("\n");
                }

                Ok(())
            }
            (bl_ast::Statement::Comment(_), _) => {
                // If the statement is a comment, we need to check if
                // the next line is the end of the line.
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
        hiding: Text, Tag, For, If, IfClause, Case, When, Comment, Inline, Body, Name, Expr, Block, With, Extends
    );

    type TagRet = ();

    /// Tags that the formatter has no layout for are kept as they are written.
    /// The match is exhaustive so that a new kind of tag has to be added here.
    fn visit_tag(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Tag>,
    ) -> Result<Self::TagRet, Self::Error> {
        match node.body() {
            bl_ast::Tag::Block(block) => self.visit_block(block.ast_ref()),
            bl_ast::Tag::With(with) => self.visit_with(with.ast_ref()),
            bl_ast::Tag::Extends(extends) => self.visit_extends(node.with_body(extends)),
            bl_ast::Tag::If(if_block) => self.visit_if(node.with_body(if_block)),
            bl_ast::Tag::For(for_loop) => self.visit_for(for_loop.ast_ref()),
            bl_ast::Tag::Case(case) => self.visit_case(case.ast_ref()),
            bl_ast::Tag::Generic(_)
            | bl_ast::Tag::Unprocessable(_)
            | bl_ast::Tag::Assignment(_)
            | bl_ast::Tag::MacroDef(_)
            | bl_ast::Tag::Include(_)
            | bl_ast::Tag::Import(_)
            | bl_ast::Tag::Continue(_)
            | bl_ast::Tag::Break(_)
            | bl_ast::Tag::Raw(_)
            | bl_ast::Tag::Render(_)
            // The body of a Liquid `capture` is rendered into a string, so its
            // whitespace matters, just like a `raw` block's.
            | bl_ast::Tag::Capture(_)
            | bl_ast::Tag::Liquid(_) => {
                // The tag starts a line when the source starts one with it.
                let range = node.range();
                let whitespace = self.whitespace_before(range.start());
                if whitespace.contains('\n') && !self.at_line_start() {
                    self.end_line();
                }

                if self.at_line_start() {
                    self.indent_kept_source(range.start(), self.ctx.source.hunk(range));
                } else {
                    self.space_from_previous(range.start());
                }

                self.push_source(range);
                Ok(())
            }
        }
    }

    type BlockRet = ();

    fn visit_block(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Block>,
    ) -> Result<Self::BlockRet, Self::Error> {
        let bl_ast::Block { label, block_body, end_label, trim } = node.body();
        let open = self.ctx.open_elements.clone();

        self.indent_tag();
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("block");
            if let Some(label) = label {
                this.push_hunk(" ");
                this.visit_name(label.ast_ref())?;
            }

            Ok(())
        })?;
        self.end_line();

        // Now visit the block body.
        self.with_block(|formatter| formatter.visit_body(block_body.ast_ref()))?;
        let after = self.ctx.open_elements.clone();

        let end = match end_label {
            Some(end_label) => {
                format!("endblock {}", self.ctx.source.hunk(end_label.ast_ref().range()))
            }
            None => "endblock".to_string(),
        };
        self.end_block(&end, trim.end, &open, after)
    }

    type WithRet = ();

    fn visit_with(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::With>,
    ) -> Result<Self::WithRet, Self::Error> {
        let bl_ast::With { assignments, block_body, trim, .. } = node.body();
        let open = self.ctx.open_elements.clone();

        // Keep the assignments as they are written, e.g. `a=1 b=2` or `x as y`.
        self.indent_tag();
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("with");
            if !assignments.is_empty() {
                this.push_hunk(" ");
                this.push_source(assignments.range());
            }

            Ok(())
        })?;
        self.end_line();

        // Now visit the block body.
        self.with_block(|formatter| formatter.visit_body(block_body.ast_ref()))?;

        let after = self.ctx.open_elements.clone();
        self.end_block("endwith", trim.end, &open, after)
    }

    type ForRet = ();

    fn visit_for(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::For>,
    ) -> Result<Self::ForRet, Self::Error> {
        let bl_ast::For {
            kind,
            target,
            iterator,
            guard,
            reverse_modifier,
            params,
            loop_body,
            loop_empty,
            trim,
            empty_trim,
        } = node.body();

        let (opening, end) = match kind {
            bl_ast::LoopKind::For => ("for", "endfor"),
            bl_ast::LoopKind::TableRow => ("tablerow", "endtablerow"),
        };
        let open = self.ctx.open_elements.clone();

        // @@ Proof of concept: for now, we will just push a for loop into the buffer.
        self.indent_tag();
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk(opening);
            this.push_hunk(" ");
            this.push_source(target.ast_ref().range());
            this.push_hunk(" in ");
            this.visit_expr(iterator.ast_ref())?;

            // Keep the modifiers in the order they are written, e.g. `reversed`
            // and `limit: 2` in Liquid, which come before the guard.
            let modifiers = reverse_modifier
                .iter()
                .map(|name| name.ast_ref().range())
                .chain((!params.is_empty()).then(|| params.range()))
                .reduce(|range, other| {
                    ByteRange::new(range.start().min(other.start()), range.end().max(other.end()))
                });

            if let Some(modifiers) = modifiers {
                this.push_hunk(" ");
                this.push_source(modifiers);
            }

            // Check if we have an if guard on the loop itself.
            if let Some(guard) = guard {
                this.push_hunk(" if ");
                this.visit_expr(guard.ast_ref())?;
            }

            Ok(())
        })?;
        self.end_line();

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(loop_body.ast_ref()))?;
        let after = self.ctx.open_elements.clone();

        // Check if we have an empty loop body, which comes after `{% empty %}`
        // in [Django] and `{% else %}` in [Jinja], [Liquid] and [Twig].
        //
        // [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
        // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
        // [Liquid]: https://shopify.github.io/liquid/tags/iteration/#else
        // [Twig]: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
        if let Some(loop_empty) = loop_empty {
            self.start_branch(&open);
            self.push_tag_line(self.ctx.dialect.empty_loop_tag(), *empty_trim)?;
            self.with_block(|formatter| formatter.visit_body(loop_empty.ast_ref()))?;
        }

        self.end_block(end, trim.end, &open, after)
    }

    type IfRet = ();

    fn visit_if(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::If>,
    ) -> Result<Self::IfRet, Self::Error> {
        let bl_ast::If { clauses, otherwise, else_trim, end_trim } = node.body();
        let open = self.ctx.open_elements.clone();
        let mut after = None;

        // Walk the clauses, and format each one of them.
        for clause in clauses.iter() {
            self.start_branch(&open);
            self.visit_if_clause(clause.ast_ref())?;
            after.get_or_insert_with(|| self.ctx.open_elements.clone());
        }

        if let Some(otherwise) = otherwise {
            self.start_branch(&open);
            self.push_tag_line("else", *else_trim)?;
            self.with_block(|formatter| formatter.visit_body(otherwise.ast_ref()))?;
        }

        // A Liquid `unless` block ends with `{% endunless %}`.
        let end = match clauses.first().map(|clause| clause.body.kind) {
            Some(bl_ast::ClauseKind::Unless) => "endunless",
            _ => "endif",
        };
        let after = after.unwrap_or_else(|| open.clone());
        self.end_block(end, *end_trim, &open, after)
    }

    type IfClauseRet = ();

    fn visit_if_clause(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::IfClause>,
    ) -> Result<Self::IfClauseRet, Self::Error> {
        let bl_ast::IfClause { kind, condition, clause_body, trim } = node.body();

        self.indent_tag();
        self.within_tag(TagKind::Block, *trim, |this| {
            match kind {
                bl_ast::ClauseKind::If => this.push_hunk("if "),
                // [Liquid] spells `elif` as `elsif`.
                //
                // [Liquid]: https://shopify.github.io/liquid/tags/control-flow/#elsif--else
                bl_ast::ClauseKind::Elif => {
                    this.push_hunk(this.ctx.dialect.elif_tag());
                    this.push_hunk(" ");
                }
                bl_ast::ClauseKind::Unless => this.push_hunk("unless "),
            }
            this.visit_expr(condition.ast_ref())
        })?;
        self.end_line();

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(clause_body.ast_ref()))?;

        Ok(())
    }

    type CaseRet = ();

    fn visit_case(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Case>,
    ) -> Result<Self::CaseRet, Self::Error> {
        let bl_ast::Case { subject, leading, branches, otherwise, trim, else_trim } = node.body();
        let open = self.ctx.open_elements.clone();
        let mut after = None;

        self.indent_tag();
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("case ");
            this.visit_expr(subject.ast_ref())
        })?;
        self.end_line();

        // Liquid doesn't render what comes before the first `{% when %}`, but
        // it is kept all the same.
        self.with_block(|formatter| formatter.visit_body(leading.ast_ref()))?;

        for branch in branches.iter() {
            self.start_branch(&open);
            self.visit_when(branch.ast_ref())?;
            after.get_or_insert_with(|| self.ctx.open_elements.clone());
        }

        if let Some(otherwise) = otherwise {
            self.start_branch(&open);
            self.push_tag_line("else", *else_trim)?;
            self.with_block(|formatter| formatter.visit_body(otherwise.ast_ref()))?;
            after.get_or_insert_with(|| self.ctx.open_elements.clone());
        }

        let after = after.unwrap_or_else(|| open.clone());
        self.end_block("endcase", trim.end, &open, after)
    }

    type WhenRet = ();

    fn visit_when(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::When>,
    ) -> Result<Self::WhenRet, Self::Error> {
        let bl_ast::When { values, clause_body, trim } = node.body();

        // The values are separated by either `,` or `or`, so keep them as they
        // are written.
        self.indent_tag();
        self.within_tag(TagKind::Block, *trim, |this| {
            this.push_hunk("when ");
            this.push_source(values.range());
            Ok(())
        })?;
        self.end_line();

        self.with_block(|formatter| formatter.visit_body(clause_body.ast_ref()))
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
            // The tags in a `<pre>` or `<textarea>` are kept exactly as they
            // are written, with the whitespace around them, since it is the
            // element's content. A block in one is kept whole, so a closing
            // tag in it doesn't close the element.
            let statement = item.ast_ref();
            if self.ctx.keeps_whitespace()
                && !matches!(statement.body(), bl_ast::Statement::Text(_))
            {
                let range = statement.range();
                self.push_exact(range.start(), self.ctx.source.hunk(range));
                continue;
            }

            walk_mut_self::walk_statement(self, item.ast_ref())?;

            // The whitespace after a statement in a `<pre>` or `<textarea>` is
            // kept as it is written, before what comes next.
            if self.ctx.keeps_whitespace() {
                continue;
            }

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
        let start = node.range().start();
        let text = self.ctx.source.hunk(node.range());

        // A `<style>` or `<script>` element that a tag splits ends in a later
        // text, at its closing tag, after which the text is HTML again.
        if let Some(end) = self.ctx.language().end_in(text) {
            self.push_text(start, &text[..end])?;
            return self.push_html(start + end, &text[end..]);
        }

        self.push_text(start, text)
    }

    type CommentRet = ();

    /// Visit a comment node.
    ///
    /// For comments, we just emit the content of the comment as a verbatim.
    fn visit_comment(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Comment>,
    ) -> Result<Self::CommentRet, Self::Error> {
        // The span of the comment includes its delimiters, which are either
        // `{# ... #}` or `{% comment %} ... {% endcomment %}`.
        self.push_source(node.range());
        self.end_line();

        Ok(())
    }

    type InlineRet = ();

    fn visit_inline(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Inline>,
    ) -> Result<Self::InlineRet, Self::Error> {
        let bl_ast::Inline { expr, trim } = node.body();
        let start = node.range().start();

        // The tag starts a line when the source starts one with it, since the
        // line break can matter, e.g. after a `//` comment in a script.
        self.start_item(start);
        self.within_tag(TagKind::Inline, *trim, |this| {
            this.visit_expr(expr.ast_ref())?;
            Ok(())
        })
    }

    type ExtendsRet = ();

    fn visit_extends(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Extends>,
    ) -> Result<Self::ExtendsRet, Self::Error> {
        let bl_ast::Extends { template, trim } = node.body();

        self.within_tag(TagKind::Block, *trim, |this| {
            this.push_hunk("extends ");
            this.visit_expr(template.ast_ref())
        })?;
        self.end_line();

        Ok(())
    }

    type NameRet = ();

    fn visit_name(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Name>,
    ) -> Result<Self::NameRet, Self::Error> {
        self.push_source(node.range());
        Ok(())
    }

    type ExprRet = ();

    /// Expressions are kept as they are written, since the dialects differ in
    /// how they write filters, arguments and operators.
    fn visit_expr(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Expr>,
    ) -> Result<Self::ExprRet, Self::Error> {
        self.push_source(node.range());
        Ok(())
    }
}
