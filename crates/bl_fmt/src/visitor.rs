use std::{borrow::Cow, ops::Range, rc::Rc};

use bl_ast::{
    AstVisitorMutSelf, ByteRange, Dialect, SourceId, Span, SpannedSource,
    ast_visitor_mut_self_default_impl, walk_mut_self,
};
use bl_lexer::{
    Lexer,
    token::{Delimiter, TokenKind},
};
use bl_reporting::{
    DiagnosticsMut,
    inline::{InlineSnippet, note_on_span},
};
use bl_workspace::config::Markup;

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, Formatted, FormatterContext, HasHTMLParsing, HtmlPart,
        LanguageType, TerminalState, Unparsed,
    },
    diagnostics::{FmtDiagnostics, FmtError, FmtResult, FmtWarning, FmtWarningKind},
    dialects::html::is_verbatim,
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
    /// the buffer then. In a `<pre>`, `<textarea>`, `<script>` or `<style>`,
    /// the source after it is pushed exactly as it is written too, from there.
    exact: Option<(usize, usize)>,

    /// What the formatter reports while it formats all of the template.
    diagnostics: FmtDiagnostics,

    /// The tags of a template that outputs text, see [Markup::Text], whose
    /// source between the tags is copied rather than laid out.
    text: Option<Splice>,
}

/// The tags of a template that outputs text, see [Markup::Text], in which the
/// whitespace between the tags is output as it is written. Each tag is
/// formatted in its place, and the source between it and the one before it
/// is copied as it is, so none of the layout applies.
struct Splice {
    /// Where each `{% %}` and `{{ }}` tag is in the source, in order.
    tags: Vec<ByteRange>,

    /// The index of the next tag in [Self::tags] that could be formatted.
    next: usize,

    /// Where in the source the formatter has copied or formatted up to.
    copied: usize,
}

impl Splice {
    /// The tags of the template in `source`, as the lexer finds them in
    /// `dialect`, without those in a tag, e.g. the lines of a Liquid
    /// `{% liquid %}` tag.
    fn new(source: SpannedSource, id: SourceId, dialect: Dialect) -> Self {
        let tokens = Lexer::new(source, id, dialect).tokenise().tokens;
        let mut tags = vec![];

        let mut index = 0;
        while let Some(token) = tokens.get(index) {
            index += 1;

            if let TokenKind::Tree(delimiter, len) = token.kind {
                if matches!(delimiter, Delimiter::Percent | Delimiter::Brace) {
                    tags.push(token.span);
                }

                index += len as usize;
            }
        }

        Splice { tags, next: 0, copied: 0 }
    }

    /// The next tag in the source, which starts after what has been copied.
    fn next_tag(&mut self) -> ByteRange {
        while self.tags.get(self.next).is_some_and(|tag| tag.start() < self.copied) {
            self.next += 1;
        }

        let tag = *self.tags.get(self.next).expect("the lexer finds each tag that the parser does");
        self.next += 1;
        tag
    }
}

/// The width that Biome wraps the lines of HTML to.
const LINE_WIDTH: usize = 80;

/// Whether the HTML that starts at `start` in `source` starts in a tag, e.g.
/// in the attributes of `<a {% if x %}href="{{ url }}"{% endif %}>`, as the
/// last `<` or `>` before it tells, outside of the template's tags.
fn starts_in_tag(source: &str, start: usize) -> bool {
    let mut before = &source[..start];

    while let Some(last) = before.rfind(['<', '>', '}']) {
        let (rest, c) = (&before[..last], &before[last..=last]);

        // Skip a template tag, from its closing delimiter back to its opening
        // one, e.g. the `>` in `{% if a > b %}`.
        before = match rest.chars().next_back() {
            Some(close @ ('}' | '%' | '#')) if c == "}" => {
                let open = if close == '}' { "{{" } else { &format!("{{{close}") };
                &rest[..rest.rfind(open).unwrap_or(0)]
            }
            _ if c == "}" => rest,
            _ => return c == "<",
        };
    }

    false
}

/// A run of HTML and `{{ }}` tags, which is formatted as one piece of HTML so
/// that its lines are wrapped with the tags in them, see
/// [Formatter::push_run]. Each tag is a word of placeholder characters as wide
/// as the tag is formatted: a character that names the tag, then
/// [Run::PADDING].
struct Run {
    /// The run, with each tag as its placeholder.
    text: String,

    /// Each tag, formatted, by its index.
    tags: Vec<String>,

    /// Where each piece of the run starts, in [Self::text] and in the source,
    /// in order. Each tag and the text between two tags is a piece.
    pieces: Vec<(usize, usize)>,
}

impl Run {
    /// The character that the placeholder of the first tag starts with. Those
    /// of the tags after it follow it, up to [Self::LAST].
    const FIRST: char = '\u{E000}';

    /// The character that the placeholder of the last tag that a run can hold
    /// starts with.
    const LAST: char = '\u{F7FF}';

    /// The character that the rest of each placeholder is made of.
    const PADDING: char = '\u{F8FF}';

    /// Whether `c` is a character of a placeholder, which are all in Unicode's
    /// Private Use Area, so that none is in a template.
    fn is_placeholder(c: char) -> bool {
        (Self::FIRST..=Self::PADDING).contains(&c)
    }

    /// Where `at` in [Self::text] is in the source, e.g. the start of a tag at
    /// the start of its placeholder.
    fn source_offset(&self, at: usize) -> usize {
        let index = self.pieces.partition_point(|&(start, _)| start <= at).saturating_sub(1);
        let (start, source) = self.pieces[index];
        source + (at - start)
    }

    /// `text`, a part of [Self::text] or its formatted code, with the tag of
    /// each placeholder in it in its place.
    fn with_tags(&self, text: &str) -> String {
        let mut with_tags = String::with_capacity(text.len());
        for c in text.chars().filter(|&c| c != Self::PADDING) {
            match c {
                Self::FIRST..=Self::LAST => {
                    with_tags.push_str(&self.tags[c as usize - Self::FIRST as usize]);
                }
                c => with_tags.push(c),
            }
        }

        with_tags
    }
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

/// A statement of a body, as the formatter walks it.
type Statement<'t> = &'t bl_ast::AstNode<'t, bl_ast::Statement<'t>>;

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
            diagnostics: FmtDiagnostics::new(),
            text: (options.markup == Markup::Text).then(|| Splice::new(source, id, dialect)),
        }
    }

    /// Convert the formatter into the buffer, and what it reported.
    pub fn finish(mut self) -> (String, FmtDiagnostics) {
        // In text, the source after the last tag is copied as it is.
        self.copy_source(self.ctx.source.source.len());

        (self.buffer, self.diagnostics)
    }

    /// Copy the source up to `end` as it is written, from where the formatter
    /// last copied or formatted up to, in a template that outputs text, see
    /// [Splice].
    fn copy_source(&mut self, end: usize) {
        let Some(text) = &mut self.text else {
            return;
        };

        if end > text.copied {
            self.buffer.push_str(&self.ctx.source.source[text.copied..end]);
            text.copied = end;
        }
    }

    /// Apply an indent to the buffer.
    #[inline(always)]
    pub fn add_indent(&mut self) {
        // The indent in text is output, so it is kept as it is written.
        if self.text.is_some() {
            return;
        }

        let size = self.ctx.indent_level();

        self.buffer.push_str(" ".repeat(size as usize).as_str());
    }

    /// Whether the next hunk would start a new line in the buffer.
    #[inline(always)]
    fn at_line_start(&self) -> bool {
        self.buffer.is_empty() || self.buffer.ends_with('\n')
    }

    /// Separate an item that continues the current line from what precedes
    /// it with a single space, when the source has one. The lexer skips
    /// whitespace and the engines trim it, which would otherwise turn
    /// `{{ a }}, {{ b }}` into `{{ a }},{{ b }}` and change the output.
    fn space_from_previous(&mut self, start: usize) {
        if self.text.is_none()
            && is_inline_space(self.ctx.source.whitespace_before(start))
            && !self.at_line_start()
            && !self.buffer.ends_with(char::is_whitespace)
        {
            self.push_hunk(" ");
        }
    }

    /// End the current line, without any spaces at its end, e.g. the one
    /// that a piece of HTML ends with before a tag that starts a line.
    pub fn end_line(&mut self) {
        // The line breaks in text are output, so they are kept as they are
        // written.
        if self.text.is_some() {
            return;
        }

        let len = self.buffer.trim_end_matches([' ', '\t']).len();
        self.buffer.truncate(len);
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

    /// Visit `expr`, which a tag holds whole, e.g. the condition of an
    /// `{% if %}`, in the parentheses that group it, e.g. `(a or b)` rather
    /// than `a or b` in `{{ (a or b) }}`. The tree has no node for them, see
    /// [bl_ast::Expr], so they are found in the source around the expression.
    fn visit_tag_expr(&mut self, expr: bl_ast::AstNodeRef<bl_ast::Expr>) -> FmtResult<()> {
        let inner = expr.range();
        let outer = self.with_parentheses(inner);

        if outer != inner {
            self.push_source(ByteRange::new(outer.start(), inner.start() - 1));
        }

        self.visit_expr(expr)?;

        if outer != inner {
            self.push_source(ByteRange::new(inner.end() + 1, outer.end()));
        }

        Ok(())
    }

    /// The range of the expression at `range` with the parentheses that group
    /// it, see [Formatter::visit_tag_expr]. Any `(` before the expression and
    /// `)` after it group it, since a tag holds it whole, see
    /// [SpannedSource::parenthesised].
    fn with_parentheses(&self, range: ByteRange) -> ByteRange {
        // Parentheses only group an expression in [Jinja], [Nunjucks] and
        // [Twig].
        //
        // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#logic
        // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#logic
        // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
        if !self.ctx.dialect.has_grouping() {
            return range;
        }

        self.ctx.source.parenthesised(range)
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

    /// Indent a tag that ends a block, or a branch of it, when it starts a
    /// line, so that it lines up with the tag that starts the block. One that
    /// continues a line, e.g. the `{% endif %}` of `{% if a %}b{% endif %}`,
    /// isn't indented.
    ///
    /// The tag doesn't know where it starts in the source, so it only starts a
    /// line of its own after source pushed exactly, see [Self::push_exact],
    /// which leaves the line break at its end for what comes next, e.g. the
    /// `{% else %}` after a `<style>` that the `{% if %}` branch opens.
    fn indent_tag(&mut self) {
        if self.exact_ends_line() && !self.at_line_start() {
            self.end_line();
        }

        if self.at_line_start() {
            self.add_indent();
        }
    }

    /// Whether the buffer ends with source pushed exactly, see
    /// [Self::push_exact], after which the source ends the line.
    fn exact_ends_line(&self) -> bool {
        self.exact.is_some_and(|(end, len)| {
            let after = &self.ctx.source.source[end..];
            len == self.buffer.len()
                && after[..after.len() - after.trim_start().len()].contains('\n')
        })
    }

    /// Start an item that starts at `start` in the source, e.g. a `{{ }}` tag:
    /// on a line of its own, at the current indent, when the source starts one
    /// with it, and otherwise after a space when the source has one.
    fn start_item(&mut self, start: usize) {
        if self.ctx.source.whitespace_before(start).contains('\n') && !self.at_line_start() {
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
    fn start_branch(&mut self, open: &[Rc<str>]) {
        self.ctx.set_open_elements(open.to_vec());
    }

    /// End a block with its end tag, at the elements `open` at its start, after
    /// which the elements `after` are open: those that its first branch leaves
    /// open, as the template would be if it took it.
    fn end_block(
        &mut self,
        end: &str,
        trim: bl_ast::TrimMarker,
        open: &[Rc<str>],
        after: Vec<Rc<str>>,
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
        // In text, the tag takes the place of the next one in the source, after
        // the source before it, which is copied as it is.
        let tag = self.text.as_mut().map(Splice::next_tag);
        if let Some(tag) = tag {
            self.copy_source(tag.start());
        }

        self.push_hunk(kind.left());
        self.push_marker(trim.left);
        self.push_hunk(" ");

        // Run F without an indent level.
        f(self)?;

        // Add the closing tag.
        self.push_hunk(" ");
        self.push_marker(trim.right);
        self.push_hunk(kind.right());

        if let (Some(text), Some(tag)) = (&mut self.text, tag) {
            text.copied = tag.end() + 1;
        }

        Ok(())
    }

    /// The contents of an `{% else %}` or end tag named `name`, with the text
    /// at `markup` after its name, which Liquid ignores, as it is written, if
    /// there is any, e.g. `else if b`, see
    /// [Dialect::else_and_end_tags_ignore_markup]. A space separates them
    /// when the source has one, so `endfor-` stays as it is, and the space
    /// that the tag ends with keeps its `-` apart from the `%}`, where it
    /// would be a whitespace control marker.
    fn with_markup(&self, name: &'static str, markup: Option<ByteRange>) -> Cow<'static, str> {
        let Some(markup) = markup else {
            return Cow::Borrowed(name);
        };

        let space =
            if self.ctx.source.whitespace_before(markup.start()).is_empty() { "" } else { " " };
        Cow::Owned(format!("{name}{space}{}", self.ctx.source.hunk(markup)))
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
        match self.ctx.state.language {
            LanguageType::Html => return self.push_html(start, text),
            // The text is only CSS or JavaScript when a tag splits a `<style>`
            // or `<script>`, whose content is kept exactly as it is written,
            // unlike a whole one's. A piece of a script can't be formatted on
            // its own without changing it: Biome drops the `;` of `{{ data }};`,
            // and makes `{{ f }}\n(x)`, a call, into `{{ f }}\nx;`.
            LanguageType::Css | LanguageType::Js => self.push_exact(start, text),
            LanguageType::Text => {
                self.push_piece(start, text, Some(Formatted::new(text.to_string())))
            }
        }

        Ok(())
    }

    /// Push the piece of HTML `text`, which starts at `start` in the source,
    /// part by part, and open and close the elements that it does.
    fn push_html(&mut self, start: usize, text: &str) -> Result<(), FmtError> {
        let parts = self.adaptor.html_engine(&self.ctx).format(text)?;
        self.push_parts(parts, start, text, None);
        Ok(())
    }

    /// Push the `parts` of the piece of HTML `text`, which starts at `start`
    /// in the source, and open and close the elements that they do. The text
    /// of a `run` is its own, whose parts hold placeholders for its tags, see
    /// [Run].
    fn push_parts(&mut self, parts: Vec<HtmlPart>, start: usize, text: &str, run: Option<&Run>) {
        let at = |offset: usize| run.map_or(start + offset, |run| run.source_offset(offset));
        let with_tags = |html: &str| run.map(|run| run.with_tags(html));

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
                    // is kept exactly as it is written, as a whole one's is.
                    let html = &text[Range::from(range)];
                    if self.ctx.in_verbatim() {
                        self.push_exact(at(range.start()), html);
                    } else {
                        let formatted =
                            formatted.map(|formatted| match with_tags(&formatted.code) {
                                Some(code) => Formatted { code, ..formatted },
                                None => formatted,
                            });
                        let kept = with_tags(html);
                        self.push_piece(
                            at(range.start()),
                            kept.as_deref().unwrap_or(html),
                            formatted,
                        );
                    }

                    self.ctx.open_elements(opens);
                }
                HtmlPart::Verbatim { range, unparsed } => {
                    self.push_exact(at(range.start()), &text[Range::from(range)]);
                    if let Some(unparsed) = unparsed {
                        self.report_unparsed(start, unparsed);
                    }
                }
                HtmlPart::Close { range, name } => {
                    // The closing tag of a `<pre>` or `<textarea>` stays where it
                    // is, as the whitespace before it is its content. Any other
                    // is at the indent of the elements that are still open.
                    let tag = &text[Range::from(range)];
                    if self.ctx.keeps_whitespace() {
                        self.push_exact(at(range.start()), tag);
                        self.ctx.close_element(&name);
                    } else {
                        self.ctx.close_element(&name);
                        self.start_item(at(range.start()));
                        self.push_hunk(tag);
                    }
                }
            }
        }
    }

    /// Push the run of text and `{{ }}` tags at `run` in `statements`, see
    /// [Self::run_at], as one piece of HTML, in which each tag is a word as
    /// wide as it is formatted, see [Run]. Biome then wraps the lines of
    /// the text with the tags in them, as it would wrap the words of the
    /// text. If each piece of text between two tags were formatted on its
    /// own, from the start of a line, a tag would lengthen the line that it
    /// continues.
    ///
    /// A run that can't be formatted as one piece, see [Self::push_run_whole],
    /// is pushed a statement at a time.
    fn push_run(&mut self, statements: &[Statement], run: Range<usize>) -> FmtResult<()> {
        if self.push_run_whole(statements, run.clone())? {
            return Ok(());
        }

        for index in run {
            self.push_statement(statements, index)?;
        }

        Ok(())
    }

    /// Push the run at `run` in `statements` as one piece of HTML, see [Run],
    /// at the width that the indent leaves. Returns `false`, having pushed
    /// nothing, if the run can't be, e.g. when it starts in a tag, or a tag is
    /// in an attribute value, see [HasHTMLParsing::format_run].
    fn push_run_whole(&mut self, statements: &[Statement], run: Range<usize>) -> FmtResult<bool> {
        let source = self.ctx.source.source;
        let (start, end) =
            (statements[run.start].range().start(), statements[run.end - 1].range().end() + 1);
        if source[start..end].contains(Run::is_placeholder) || starts_in_tag(source, start) {
            return Ok(false);
        }

        let mut text = String::with_capacity(end - start);
        let (mut tags, mut words, mut pieces) = (vec![], vec![], vec![(0, start)]);
        let mut from = start;

        for statement in &statements[run.clone()] {
            let bl_ast::Statement::Inline(inline) = statement.body() else {
                continue;
            };

            let Some(first) = char::from_u32(Run::FIRST as u32 + tags.len() as u32)
                .filter(|&first| first <= Run::LAST)
            else {
                return Ok(false);
            };

            // A tag in a tag, e.g. in an attribute value, isn't a word of text,
            // which the last `<` or `>` before it tells, as the run doesn't
            // start in a tag. Finding it here saves parsing the run.
            let range = statement.range();
            text.push_str(&source[from..range.start()]);
            if text.rfind(['<', '>']).is_some_and(|last| text[last..].starts_with('<')) {
                return Ok(false);
            }

            pieces.push((text.len(), range.start()));

            // Format the tag on its own, to know how wide it is.
            let bl_ast::Inline { expr, trim } = inline;
            let len = self.buffer.len();
            self.within_tag(TagKind::Inline, *trim, |this| this.visit_tag_expr(expr.ast_ref()))?;
            let tag = self.buffer.split_off(len);

            let word = text.len();
            text.push(first);
            text.extend(std::iter::repeat_n(Run::PADDING, tag.chars().count() - 1));
            words.push(word..text.len());
            tags.push(tag);

            from = range.end() + 1;
            pieces.push((text.len(), from));
        }

        text.push_str(&source[from..end]);

        let width = LINE_WIDTH.saturating_sub(self.ctx.indent_level() as usize).max(LINE_WIDTH / 2);
        let parts = self.adaptor.html_engine(&self.ctx).format_run(&text, &words, width)?;
        let Some(parts) = parts else {
            return Ok(false);
        };

        let whole = Run { text, tags, pieces };
        self.push_parts(parts, start, &whole.text, Some(&whole));

        let last = statements[run.end - 1].ast_ref();
        let next = statements.get(run.end).map(|next| next.ast_ref());
        self.check_inline_statement_newline(last, next)?;
        Ok(true)
    }

    /// Report the code of a `<script>` or `<style>` that doesn't parse, in the
    /// piece of HTML that starts at `start` in the source.
    fn report_unparsed(&mut self, start: usize, unparsed: Unparsed) {
        let Unparsed { language, range, message } = unparsed;
        let range = ByteRange::new(start + range.start(), start + range.end());
        let span = Span::new(range, self.ctx.id());

        let kind = FmtWarningKind::UnparsedCode { language, message };
        self.diagnostics.add_warning(FmtWarning::new(kind, span));
    }

    /// Push `text`, which starts at `start` in the source, exactly as it is
    /// written, e.g. the content of a `<pre>`, but for the whitespace at its
    /// end when it ends a line, which the formatter lays out with what comes
    /// next, e.g. a `</script>`. The last line is left open, as
    /// [Self::push_lines] leaves it.
    ///
    /// In a `<pre>`, `<textarea>`, `<script>` or `<style>`, the whitespace
    /// before `text` is pushed exactly too: from where the buffer ends with
    /// source pushed exactly, or else the whitespace right before `start`.
    /// When the buffer starts a line, e.g. after the `{% endif %}` of a block
    /// whose branches open a `<style>`, it already has the line break that the
    /// whitespace starts with, which ended the line of the tag.
    fn push_exact(&mut self, start: usize, text: &str) {
        let mut from = match self.exact.filter(|&(_, len)| len == self.buffer.len()) {
            _ if !self.ctx.in_verbatim() => start,
            Some((end, _)) => end,
            None => start - self.ctx.source.whitespace_before(start).len(),
        };

        let end = start + text.len();
        if self.at_line_start() {
            let source = &self.ctx.source.source[from..end];
            let whitespace = &source[..source.len() - source.trim_start().len()];
            if let Some(newline) = whitespace.find('\n') {
                from += newline + 1;
            }
        }

        let source = &self.ctx.source.source[from..end];
        let trimmed = source.trim_end();
        let source = if source[trimmed.len()..].contains('\n') { trimmed } else { source };

        self.push_hunk(source);
        self.exact = Some((from + source.len(), self.buffer.len()));
    }

    /// Push `text`, which starts at `start` in the source, as `formatted`, or
    /// as it is written if that is `None`.
    fn push_piece(&mut self, start: usize, text: &str, formatted: Option<Formatted>) {
        let start = start + (text.len() - text.trim_start().len());

        // The text starts a line when the source starts one with it.
        if !text.trim().is_empty()
            && self.ctx.source.whitespace_before(start).contains('\n')
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
    /// left open. A line that `formatted` keeps as it is written, e.g. the
    /// second line of a comment, has the indent of the source instead, which
    /// indenting again would add to each time the template is formatted.
    fn push_lines(&mut self, formatted: &Formatted) {
        let mut lines: Vec<_> = formatted.code.lines().collect();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }

        for (index, line) in lines.into_iter().enumerate() {
            if index > 0 {
                self.end_line();
            }

            if self.at_line_start() && !line.is_empty() && !formatted.keeps_line(index) {
                self.add_indent();
            }

            self.push_hunk(line);
        }
    }

    /// Push the source at `range` as it is written, e.g. a comment, or a tag
    /// that the formatter has no layout for. It starts a line when the source
    /// starts one with it, and otherwise follows what comes before it after a
    /// space when the source has one.
    fn push_kept_source(&mut self, range: ByteRange) {
        if self.text.is_some() {
            return self.copy_source(range.end() + 1);
        }

        let whitespace = self.ctx.source.whitespace_before(range.start());
        if whitespace.contains('\n') && !self.at_line_start() {
            self.end_line();
        }

        if self.at_line_start() {
            self.indent_kept_source(range.start(), self.ctx.source.hunk(range));
        } else {
            self.space_from_previous(range.start());
        }

        self.push_source(range);
    }

    /// Indent `source`, which starts at `start` and a line, and which is kept
    /// as it is written. When it spans several lines, it keeps its column, so
    /// that its other lines still line up with it. Otherwise, it gets the
    /// current indent.
    fn indent_kept_source(&mut self, start: usize, source: &str) {
        let whitespace = self.ctx.source.whitespace_before(start);

        match whitespace.rfind('\n') {
            Some(newline) if source.contains('\n') => self.push_hunk(&whitespace[newline + 1..]),
            _ => self.add_indent(),
        }
    }

    /// Push the statement at `index` in `statements`, and end its line if it
    /// ends one, as the statement after it decides.
    fn push_statement(&mut self, statements: &[Statement], index: usize) -> FmtResult<()> {
        // The tags in a `<pre>`, `<textarea>`, `<script>` or `<style>` are
        // kept exactly as they are written, with the whitespace around
        // them, as is the rest of its content. A block in one is kept
        // whole, so a closing tag in it doesn't close the element.
        let statement = statements[index].ast_ref();
        if self.ctx.in_verbatim() && !matches!(statement.body(), bl_ast::Statement::Text(_)) {
            let range = statement.range();
            self.push_exact(range.start(), self.ctx.source.hunk(range));
            return Ok(());
        }

        walk_mut_self::walk_statement(self, statement)?;

        // The whitespace after a statement in a `<pre>`, `<textarea>`,
        // `<script>` or `<style>` is kept as it is written, before what
        // comes next.
        if self.ctx.in_verbatim() {
            return Ok(());
        }

        let next = statements.get(index + 1).map(|next| next.ast_ref());
        self.check_inline_statement_newline(statement, next)
    }

    /// The length of the block of text to translate that `statements` start
    /// with, from its opening tag to its closing one, see
    /// [Dialect::translation_tags], or zero if they don't start with one.
    fn translation_block_at(&self, statements: &[Statement]) -> usize {
        // A tag is named by the word after its `{%` and its whitespace control,
        // whichever way it is parsed, e.g. `{% blocktrans count n as num %}` as
        // an assignment.
        let name = |statement: &Statement| {
            let bl_ast::Statement::Tag(_) = statement.body() else {
                return None;
            };

            let tag = self.ctx.source.hunk(statement.range()).strip_prefix("{%")?;
            let tag = tag.trim_start_matches(['-', '+', '~']).trim_start();
            Some(&tag[..tag.find(|c: char| !c.is_alphanumeric() && c != '_')?])
        };

        let Some(open) = statements.first().and_then(name) else {
            return 0;
        };

        let tags = self.ctx.dialect.translation_tags();
        let Some(&(_, end)) = tags.iter().find(|&&(start, _)| start == open) else {
            return 0;
        };

        // The block ends at the next tag of the same name, which is its end tag
        // unless the tag doesn't open one, e.g. `{% trans 'Remove' %}` in Twig.
        let next = statements[1..]
            .iter()
            .position(|statement| name(statement).is_some_and(|name| name == open || name == end));
        match next {
            Some(at) if name(&statements[at + 1]) == Some(end) => at + 2,
            _ => 0,
        }
    }

    /// The length of the run of text and `{{ }}` tags that `statements` start
    /// with, which [Self::push_run] lays out together: in HTML, outside of a
    /// `<pre>`, `<textarea>`, `<script>` or `<style>`, with tags that each fit
    /// on a line, and with a tag and some text. Otherwise, it is zero.
    fn run_at(&self, statements: &[Statement]) -> usize {
        // A `<code>` in a `<pre>` is in the content of the `<pre>` too.
        if self.text.is_some()
            || self.ctx.open_elements.iter().any(|name| is_verbatim(name))
            || !matches!(self.ctx.language(), LanguageType::Html)
        {
            return 0;
        }

        let source = self.ctx.source;
        let len = statements
            .iter()
            .take_while(|statement| match statement.body() {
                bl_ast::Statement::Text(_) => true,
                bl_ast::Statement::Inline(_) => !source.hunk(statement.range()).contains('\n'),
                _ => false,
            })
            .count();

        let run = &statements[..len];
        let has_tag = run.iter().any(|item| matches!(item.body(), bl_ast::Statement::Inline(_)));
        let has_text = run.iter().any(|item| {
            matches!(item.body(), bl_ast::Statement::Text(_))
                && !source.hunk(item.range()).trim().is_empty()
        });

        if has_tag && has_text { len } else { 0 }
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
            // A comment starts a line only when the source starts one with it,
            // see [Self::push_kept_source].
            (bl_ast::Statement::Text(_), Some(bl_ast::Statement::Comment(_))) => Ok(()),

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
                let ends_line = self.ctx.source.whitespace_after(statement.range()).contains('\n');

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
                    self.end_line();
                }

                Ok(())
            }
            // A comment that ends a line in the source ends it here too.
            (bl_ast::Statement::Comment(_), _) => {
                if self.ctx.source.whitespace_after(statement.range()).contains('\n')
                    && !self.at_line_start()
                {
                    self.end_line();
                }

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
        hiding: Text, Tag, For, If, IfClause, Case, When, Guard, Comment, Inline, Body, Name, Expr, Block, With, Extends
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
            bl_ast::Tag::If(if_block) => self.visit_if(if_block.ast_ref()),
            bl_ast::Tag::For(for_loop) => self.visit_for(for_loop.ast_ref()),
            bl_ast::Tag::Case(case) => self.visit_case(case.ast_ref()),
            bl_ast::Tag::Guard(guard) => self.visit_guard(guard.ast_ref()),
            bl_ast::Tag::Generic(_)
            | bl_ast::Tag::Unprocessable(_)
            | bl_ast::Tag::BlockShortcut(_)
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
                self.push_kept_source(node.range());
                Ok(())
            }
        }
    }

    type BlockRet = ();

    fn visit_block(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Block>,
    ) -> Result<Self::BlockRet, Self::Error> {
        let bl_ast::Block { label, scoped, required, block_body, end_label, trim } = node.body();
        let open = self.ctx.open_elements.clone();

        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("block");
            if let Some(label) = label {
                this.push_hunk(" ");
                this.visit_name(label.ast_ref())?;
            }

            if *scoped {
                this.push_hunk(" scoped");
            }

            if *required {
                this.push_hunk(" required");
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
        let bl_ast::With { assignments, value, only, block_body, trim, .. } = node.body();
        let open = self.ctx.open_elements.clone();

        // Keep the assignments as they are written, e.g. `a=1 b=2` or `x as y`.
        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("with");
            if let Some(value) = value {
                this.push_hunk(" ");
                this.visit_tag_expr(value.ast_ref())?;
                if *only {
                    this.push_hunk(" only");
                }
            } else if !assignments.is_empty() {
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
            empty_markup,
            end_markup,
        } = node.body();

        let (opening, end) = match kind {
            bl_ast::LoopKind::For => ("for", "endfor"),
            bl_ast::LoopKind::TableRow => ("tablerow", "endtablerow"),
        };
        let open = self.ctx.open_elements.clone();

        // @@ Proof of concept: for now, we will just push a for loop into the buffer.
        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk(opening);
            this.push_hunk(" ");
            this.push_source(target.ast_ref().range());
            this.push_hunk(" in ");
            this.visit_tag_expr(iterator.ast_ref())?;

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
                this.visit_tag_expr(guard.ast_ref())?;
            }

            Ok(())
        })?;
        self.end_line();

        // Now visit the loop body.
        self.with_block(|formatter| formatter.visit_body(loop_body.ast_ref()))?;
        let after = self.ctx.open_elements.clone();

        // Check if we have an empty loop body, which comes after `{% empty %}`
        // in [Django] and `{% else %}` in [Jinja], [Liquid], [Nunjucks] and
        // [Twig].
        //
        // [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
        // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
        // [Liquid]: https://shopify.github.io/liquid/tags/iteration/#else
        // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#for
        // [Twig]: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
        if let Some(loop_empty) = loop_empty {
            self.start_branch(&open);
            let empty = self.with_markup(self.ctx.dialect.empty_loop_tag(), *empty_markup);
            self.push_tag_line(&empty, *empty_trim)?;
            self.with_block(|formatter| formatter.visit_body(loop_empty.ast_ref()))?;
        }

        let end = self.with_markup(end, *end_markup);
        self.end_block(&end, trim.end, &open, after)
    }

    type IfRet = ();

    fn visit_if(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::If>,
    ) -> Result<Self::IfRet, Self::Error> {
        let bl_ast::If { clauses, otherwise, else_trim, end_trim, else_markup, end_markup } =
            node.body();
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
            self.push_tag_line(&self.with_markup("else", *else_markup), *else_trim)?;
            self.with_block(|formatter| formatter.visit_body(otherwise.ast_ref()))?;
        }

        // A Liquid `unless` block ends with `{% endunless %}`.
        let end = match clauses.first().map(|clause| clause.body.kind) {
            Some(bl_ast::ClauseKind::Unless) => "endunless",
            _ => "endif",
        };
        let end = self.with_markup(end, *end_markup);
        let after = after.unwrap_or_else(|| open.clone());
        self.end_block(&end, *end_trim, &open, after)
    }

    type IfClauseRet = ();

    fn visit_if_clause(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::IfClause>,
    ) -> Result<Self::IfClauseRet, Self::Error> {
        let bl_ast::IfClause { kind, condition, clause_body, trim, elseif } = node.body();

        // Each clause spans from the start of its block, so only the first one
        // knows where its tag starts in the source.
        match kind {
            bl_ast::ClauseKind::Elif => self.indent_tag(),
            bl_ast::ClauseKind::If | bl_ast::ClauseKind::Unless => {
                self.start_item(node.range().start());
            }
        }
        self.within_tag(TagKind::Block, *trim, |this| {
            match kind {
                bl_ast::ClauseKind::If => this.push_hunk("if "),
                // [Liquid] spells `elif` as `elsif`, and [Nunjucks] spells it
                // `elseif` too, which is kept as it is written.
                //
                // [Liquid]: https://shopify.github.io/liquid/tags/control-flow/#elsif--else
                // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#if
                bl_ast::ClauseKind::Elif => {
                    this.push_hunk(if *elseif { "elseif" } else { this.ctx.dialect.elif_tag() });
                    this.push_hunk(" ");
                }
                bl_ast::ClauseKind::Unless => this.push_hunk("unless "),
            }
            this.visit_tag_expr(condition.ast_ref())
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
        let bl_ast::Case { subject, leading, branches, otherwise, trim, else_trim, end_markup } =
            node.body();
        let open = self.ctx.open_elements.clone();
        let mut after = None;

        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("case ");
            this.visit_tag_expr(subject.ast_ref())
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

        let end = self.with_markup("endcase", *end_markup);
        let after = after.unwrap_or_else(|| open.clone());
        self.end_block(&end, trim.end, &open, after)
    }

    type WhenRet = ();

    fn visit_when(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::When>,
    ) -> Result<Self::WhenRet, Self::Error> {
        let bl_ast::When { values, clause_body, trim } = node.body();

        // The values are separated by either `,` or `or`, so keep them as they
        // are written.
        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, *trim, |this| {
            this.push_hunk("when ");
            this.push_source(values.range());
            Ok(())
        })?;
        self.end_line();

        self.with_block(|formatter| formatter.visit_body(clause_body.ast_ref()))
    }

    type GuardRet = ();

    /// A Twig [`guard`] block, whose name is kept as it is written, e.g. the
    /// two words of `{% guard test same as %}`.
    ///
    /// [`guard`]: https://twig.symfony.com/doc/3.x/tags/guard.html
    fn visit_guard(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Guard>,
    ) -> Result<Self::GuardRet, Self::Error> {
        let bl_ast::Guard { kind, name, guard_body, otherwise, trim, else_trim } = node.body();
        let open = self.ctx.open_elements.clone();

        self.start_item(node.range().start());
        self.within_tag(TagKind::Block, trim.start, |this| {
            this.push_hunk("guard ");
            this.push_hunk(kind.as_str());
            this.push_hunk(" ");
            this.visit_name(name.ast_ref())
        })?;
        self.end_line();

        self.with_block(|formatter| formatter.visit_body(guard_body.ast_ref()))?;
        let after = self.ctx.open_elements.clone();

        if let Some(otherwise) = otherwise {
            self.start_branch(&open);
            self.push_tag_line("else", *else_trim)?;
            self.with_block(|formatter| formatter.visit_body(otherwise.ast_ref()))?;
        }

        self.end_block("endguard", trim.end, &open, after)
    }

    type BodyRet = ();

    fn visit_body(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Body>,
    ) -> Result<Self::BodyRet, Self::Error> {
        let bl_ast::Body { contents } = node.body();
        let statements: Vec<_> = contents.iter().collect();

        // Visit the body of the document. A run of text and `{{ }}` tags in
        // HTML is laid out together, so that its lines are wrapped with the
        // tags in them.
        let mut i = 0;
        while i < statements.len() {
            // A block of text to translate, as in [Django] and [Jinja], and in
            // Twig as [Symfony] defines it, is kept as it is written, since its
            // whitespace is part of the message that it is translated by.
            //
            // [Django]: https://docs.djangoproject.com/en/stable/topics/i18n/translation/#blocktranslate-template-tag
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#i18n
            // [Symfony]: https://github.com/symfony/symfony/blob/v7.3.0/src/Symfony/Bridge/Twig/Node/TransNode.php#L109-L129
            let block = self.translation_block_at(&statements[i..]);
            if block > 0 && !self.ctx.in_verbatim() {
                let (open, end) = (statements[i].range(), statements[i + block - 1].range());
                self.push_kept_source(ByteRange::new(open.start(), end.end()));

                i += block;
                let next = statements.get(i).map(|next| next.ast_ref());
                self.check_inline_statement_newline(statements[i - 1].ast_ref(), next)?;
                continue;
            }

            let run = self.run_at(&statements[i..]);
            if run > 1 {
                self.push_run(&statements, i..i + run)?;
                i += run;
            } else {
                self.push_statement(&statements, i)?;
                i += 1;
            }
        }

        Ok(())
    }

    type TextRet = ();

    fn visit_text(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Text>,
    ) -> Result<Self::TextRet, Self::Error> {
        // Text in a template that outputs text is copied as it is written,
        // with the source before the next tag, see [Splice].
        if self.text.is_some() {
            return Ok(());
        }

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

    /// A comment is kept as it is written, as is a tag that the formatter has
    /// no layout for.
    fn visit_comment(
        &mut self,
        node: bl_ast::AstNodeRef<bl_ast::Comment>,
    ) -> Result<Self::CommentRet, Self::Error> {
        // The span of the comment includes its delimiters, which are either
        // `{# ... #}` or `{% comment %} ... {% endcomment %}`.
        self.push_kept_source(node.range());

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
            this.visit_tag_expr(expr.ast_ref())?;
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
            this.visit_tag_expr(template.ast_ref())
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

#[cfg(test)]
mod tests {
    use super::starts_in_tag;

    #[test]
    fn html_starts_in_a_tag_after_its_name() {
        let starts_at = |source: &str| starts_in_tag(source, source.find('|').unwrap());

        assert!(starts_at("<a {% if x %}|href=\"{{ url }}\""));
        assert!(starts_at("<a {% if a > b %}|"));
        assert!(starts_at("<{{ name }} {{ attrs }}|"));
        assert!(!starts_at("<p>{% if a > b %}|"));
        assert!(!starts_at("<p>{{ a }} {# > #}|"));
        assert!(!starts_at("|"));
    }
}
