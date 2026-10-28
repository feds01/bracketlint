//! The parser module is responsible for converting a stream of tokens into an
//! abstract syntax tree.
//!
//! Based on several documentation sources, such as the [Jinja templates]
//! docs. Each dialect rule links its own spec, see `bl_ast::Dialect`.
//!
//! [Jinja templates]: https://jinja.palletsprojects.com/en/stable/templates/

use std::cell::Cell;

use bl_arena::{Arena, ArenaVec};
use bl_ast::{
    self as ast, AstNode, AstNodes, ByteRange, ForeignSyntax, Identifier, Keyword, SourceId, Span,
    SpannedSource, Syntax, VarExpr,
};
use bl_lexer::{
    kw, tok,
    token::{Delimiter, NumberFlags, Token, TokenKind, cursor::TokenCursor},
};
use bl_reporting::{
    HasDiagnosticsMut,
    inline::{InlineSnippet, note_on_span},
};
use derive_more::Deref;

use crate::{
    ParseOptions,
    diagnostics::{
        ParseResult, ParserDiagnostics,
        error::{ParseError, ParseErrorKind},
        expected::ExpectedItem,
        warning::{ParseWarning, ParseWarningKind},
    },
};

/// The context of the tag that the parser is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagContext {
    /// When the parser has encountered an `if` block.
    If,

    /// When the parser has encountered a `block` tag.
    Block,

    /// With the parser has encountered a `with`.
    With,

    /// When the parser has encountered a `comment`.
    Comment,

    /// When the parser has encountered a `raw` block.
    Raw,

    /// When the parser has encountered a `for` loop.
    For,

    /// When the parser has encountered a Liquid `case` block.
    Case,

    /// When the parser has encountered a Liquid `capture` block.
    Capture,

    /// When the parser has encountered a Twig `guard` block.
    Guard,
}
impl TagContext {
    fn applies_to(&self, kwd: Keyword) -> bool {
        match self {
            TagContext::If => matches!(kwd, Keyword::Else | Keyword::Elif),
            TagContext::Block => matches!(kwd, Keyword::EndBlock),
            TagContext::With => matches!(kwd, Keyword::EndWith),
            TagContext::Comment => matches!(kwd, Keyword::EndComment),
            TagContext::Raw => matches!(kwd, Keyword::EndRaw),
            TagContext::For => {
                matches!(kwd, Keyword::EndFor | Keyword::EndTableRow)
            }
            TagContext::Case => matches!(kwd, Keyword::When | Keyword::Else),
            TagContext::Capture => matches!(kwd, Keyword::EndCapture),
            TagContext::Guard => matches!(kwd, Keyword::EndGuard),
        }
    }
}

#[derive(Deref)]
pub struct ParseFrame<'s> {
    /// The token cursor for the current frame.
    #[deref]
    cursor: TokenCursor<'s>,

    /// If the current frame has an error.
    error: Cell<bool>,

    /// An optional tag context that can be used to determine parsing
    /// behaviour.
    tag_context: Option<TagContext>,

    /// The delimiter of the tree that the frame is in, if any.
    delimiter: Option<Delimiter>,
}

impl<'s> ParseFrame<'s> {
    pub fn from_stream(stream: &'s [Token], span: ByteRange) -> Self {
        Self {
            error: Cell::new(false),
            cursor: TokenCursor::new(stream, span),
            tag_context: None,
            delimiter: None,
        }
    }

    /// Skip `n` number of tokens.
    #[inline(always)]
    pub(crate) fn skip(&self, n: u8) {
        unsafe { self.cursor.skip(n) }
    }

    /// Set the position of the offset.
    #[inline(always)]
    pub(crate) fn set_pos(&self, pos: usize) {
        unsafe { self.cursor.set_pos(pos) }
    }

    /// Get the current location from the current token, if there is no token at
    /// the current offset, then the location of the last token is used.
    pub(crate) fn _current_pos(&self) -> ByteRange {
        // If there are no tokens in the cursor, or if current position
        // is beyond the length of the stream, then we use the last token's
        // location.
        if self.cursor.is_empty() || self.position() >= self.len() {
            return self.cursor.range();
        }

        self.current_token().span
    }

    /// Get a [ByteRange] to use when the parser encounters an un-expected end
    /// of file error. The strategy:
    ///
    /// First step, get the last "good" span.
    ///
    /// - Attempt to get the current token, if so use the span of the token.
    ///
    /// - Attempt to get the previous token, if so use the span of the token.
    ///
    /// - Fallback onto use the generator span. (infallible)
    ///
    /// Second step, offset the end of the span by one.
    fn eof_pos(&self) -> ByteRange {
        let pos = match self.peek() {
            Some(token) => token.span,
            None => self.previous_pos(),
        }
        .end()
            + 1;

        ByteRange::new(pos, pos)
    }

    /// Get a [ByteRange] to use when the parser expected a token or some other
    /// construct, but instead received something else. The strategy for
    /// getting this span is simpler:
    ///
    /// - Attempt to get the previous token, if so use the span of the token.
    ///
    /// - Fallback onto use the generator span. (infallible)
    ///
    /// - Add one to the end of the span.
    fn expected_pos(&self) -> ByteRange {
        let pos = self.previous_pos().end() + 1;
        ByteRange::new(pos, pos)
    }
}

/// The parser itself, which is responsible for converting a stream of tokens
/// into an abstract syntax tree.
#[derive(Deref)]
pub struct Parser<'s, 'a> {
    /// The [SourceId] of the source that the parser is currently parsing.
    id: SourceId,

    /// The arena that the parser allocates the tree in.
    arena: &'a Arena,

    /// The source that the parser is currently parsing. A useful wrapper and
    /// utility for the parser to access the source, i.e. especially when
    /// reporting errors.
    _source: SpannedSource<'s>,

    /// The current frame of the parser. A frame represents a particular
    /// token stream, like the file, or a subtree within the source, i.e.
    /// ```ignore
    /// {% some_tag ... %}
    /// ```
    ///
    /// The frame would represent the token stream between the `{%` and `%}`.
    #[deref]
    frame: ParseFrame<'s>,

    /// Collected diagnostics for the current [Parser].
    pub(crate) diagnostics: &'s mut ParserDiagnostics,

    /// Any options that are supplied to the parser to alter
    /// its mode of operation.
    options: ParseOptions,

    /// Syntax of another dialect that the parser stopped at, with the span of
    /// its token, see [Parser::mark_foreign]. An error at the token names it.
    foreign: Option<(ByteRange, ForeignSyntax)>,
}

impl<'s, 'a> Parser<'s, 'a> {
    pub fn new(
        id: SourceId,
        arena: &'a Arena,
        source: SpannedSource<'s>,
        stream: &'s [Token],
        diagnostics: &'s mut ParserDiagnostics,
        options: ParseOptions,
    ) -> Self {
        // We compute the `parent_span` from the given stream.
        // If the stream has no tokens, then we assume that the
        // byte range is empty.
        let parent_span = match (stream.first(), stream.last()) {
            (Some(first), Some(last)) => first.span.join(last.span),
            _ => ByteRange::default(),
        };

        Self {
            id,
            arena,
            _source: source,
            diagnostics,
            options,
            foreign: None,
            frame: ParseFrame::from_stream(stream, parent_span),
        }
    }

    pub fn current_pos(&self) -> ByteRange {
        // If there are no tokens in the cursor, or if current position
        // is beyond the length of the stream, then we use the last token's
        // location.
        if self.cursor.is_empty() || self.position() >= self.len() {
            return self.cursor.range();
        }

        self.current_token().span
    }

    /// Function to create a [Span] from a [ByteRange] by using the
    /// provided resolver
    pub(crate) fn make_span(&self, range: ByteRange) -> Span {
        Span { range, id: self.id }
    }

    /// Report an error to the parser diagnostics.
    ///
    /// This function is used to report an error to the parser diagnostics.
    #[inline(always)]
    pub(crate) fn _note_on_span(&self, span: ByteRange, note: impl Into<String>) {
        note_on_span(InlineSnippet::new(&self._source, self.make_span(span)), note.into());
    }

    /// Create a new [AstNode] from the information provided by the [AstGen]
    pub fn node_with_span<T>(&self, inner: T, location: ByteRange) -> AstNode<'a, T> {
        AstNode::new(self.arena.alloc(inner), location)
    }

    /// Create a new [AstNode] with a span that ranges from the start
    /// [ByteRange] to join with the [ByteRange].
    pub(crate) fn node_with_joined_span<T>(&self, body: T, start: ByteRange) -> AstNode<'a, T> {
        // We get the previous token, before the current since we want to
        // know the span up to the current token, not including it.
        AstNode::new(self.arena.alloc(body), start.join(self.previous_pos()))
    }

    /// Create [AstNodes] with a span.
    pub(crate) fn nodes_with_span<T>(
        &self,
        nodes: ArenaVec<'a, AstNode<'a, T>>,
        location: ByteRange,
    ) -> AstNodes<'a, T> {
        AstNodes::new(nodes.into_slice(), location)
    }

    /// Create [AstNodes] with a span that ranges from the start [ByteRange] to
    /// the current [ByteRange].
    pub(crate) fn nodes_with_joined_span<T>(
        &self,
        nodes: ArenaVec<'a, AstNode<'a, T>>,
        start: ByteRange,
    ) -> AstNodes<'a, T> {
        AstNodes::new(nodes.into_slice(), start.join(self.previous_pos()))
    }

    /// Create an error without wrapping it in an [Err] variant
    #[inline(always)]
    fn make_err(
        &self,
        kind: ParseErrorKind,
        expected: ExpectedItem,
        received: Option<TokenKind>,
        span: Option<ByteRange>,
    ) -> ParseError {
        self.frame.error.set(true);

        let range = span.unwrap_or_else(|| self.eof_pos());
        let foreign = self.foreign_at(range, received);
        ParseError::new(kind, self.make_span(range), expected, received, foreign)
    }

    /// The syntax of another dialect that an error at `range` is at, if any:
    /// syntax that the parser stopped at, see [Parser::mark_foreign], or a
    /// whitespace control marker of another dialect. An error that didn't
    /// receive a token is at the end of the tag, and so after its last token,
    /// e.g. the `~` in `{{ x ~}}`, which is Jinja's concatenation.
    fn foreign_at(&self, range: ByteRange, received: Option<TokenKind>) -> Option<ForeignSyntax> {
        if let Some((at, foreign)) = self.foreign
            && at == range
        {
            return Some(foreign);
        }

        let token = if received.is_some() { range } else { self.previous_pos() };
        self.foreign_trim_marker(token)
    }

    /// The whitespace control marker of another dialect that the token at
    /// `range` is, if it is one, e.g. the `~` in `{%~ if x %}` in Jinja. The
    /// markers of [Jinja], [Liquid], [Nunjucks] and [Twig] are written right
    /// inside a tag's delimiters, and the lexer leaves out the dialect's own,
    /// see [Parser::trim].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#whitespace-control
    /// [Liquid]: https://shopify.github.io/liquid/basics/whitespace/
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#whitespace-control
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#whitespace-control
    fn foreign_trim_marker(&self, range: ByteRange) -> Option<ForeignSyntax> {
        // The range of an error can be made up, e.g. one byte after the last
        // token, so it is read as bytes, which a character can't split.
        let source = self._source.source.as_bytes();
        let &marker = source.get(range.start()).filter(|_| range.len() == 1)?;

        let before = &source[..range.start()];
        let after = source.get(range.end() + 1..).unwrap_or_default();
        let next_to_delimiter = before.ends_with(b"{%")
            || before.ends_with(b"{{")
            || after.starts_with(b"%}")
            || after.starts_with(b"}}");

        next_to_delimiter
            .then(|| self.options.dialect.foreign(Syntax::TrimMarker(marker)))
            .flatten()
    }

    /// Mark the token at `range` as `syntax` of another dialect, if the dialect
    /// doesn't have it, so that an error at the token names the dialects that
    /// do. The parser stops at such a token, which is then usually left over.
    fn mark_foreign(&mut self, range: ByteRange, syntax: Syntax) {
        if let Some(foreign) = self.options.dialect.foreign(syntax) {
            self.foreign = Some((range, foreign));
        }
    }

    /// Mark the next token as `syntax` of another dialect, see
    /// [Parser::mark_foreign].
    fn stop_at_foreign(&mut self, syntax: Syntax) {
        if let Some(token) = self.peek().copied() {
            self.mark_foreign(token.span, syntax);
        }
    }

    /// Mark the next token as Liquid's syntax for a tag's arguments, if it is a
    /// `,` between them, or the `:` of a `name: value` argument, as in Liquid's
    /// [`render`]. The other dialects have neither.
    ///
    /// [`render`]: https://shopify.github.io/liquid/tags/template/#render-parameters
    fn stop_at_foreign_args(&mut self) {
        if !self.frame.delimiter.is_some_and(|delimiter| delimiter.is_tag()) {
            return;
        }

        match self.peek_kind() {
            Some(TokenKind::Comma) => self.stop_at_foreign(Syntax::ArgCommas),
            Some(TokenKind::Colon) => self.stop_at_foreign(Syntax::ColonArgs),
            _ => {}
        }
    }

    /// Warn about each `{# #}` that starts in the text at `range`, in a dialect
    /// without such comments, where it is text, since it is likely meant as a
    /// comment, as in [Django], [Jinja], [Nunjucks] and [Twig]. The contents of
    /// a `{% raw %}` block are output as they are written, so they are left
    /// out.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/ref/templates/language/#comments
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#comments
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#comments
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#comments
    fn warn_hash_comments(&mut self, range: ByteRange) {
        let Some(foreign) = self.options.dialect.foreign(Syntax::HashComments) else {
            return;
        };

        if self.tag_context() == Some(TagContext::Raw) {
            return;
        }

        // Each `{` is found by a search for a byte, rather than each `{#` by a
        // search for a string, which costs more to set up for each text. A
        // comment can end after the text, e.g. `{# {{ x }} #}`, so its end is
        // looked for in the rest of the source.
        let source = self._source.source;
        let mut start = range.start();
        while let Some(index) = source[start..=range.end()].find('{') {
            let open = start + index;
            start = open + 1;
            if source.as_bytes().get(start) != Some(&b'#') {
                continue;
            }

            let Some(len) = source[open + "{#".len()..].find("#}") else {
                return;
            };

            let close = open + "{#".len() + len + "#}".len();
            let span = self.make_span(ByteRange::new(open, close - 1));
            self.add_warning(ParseWarning::new(ParseWarningKind::HashComment(foreign), span));

            if close > range.end() {
                return;
            }
            start = close;
        }
    }

    /// Warn about a tag that is named as another dialect names one of its
    /// tags, since the template is likely written in that dialect:
    ///
    /// - The tag that starts an `elif` clause, which is `elsif` in
    ///   [Liquid][Liquid `elsif`], `elseif` in [Twig][Twig `if`], `elif` in
    ///   [Django][Django `if`] and [Jinja][Jinja `if`], and either of the last
    ///   two in [Nunjucks][Nunjucks `if`].
    /// - The tags of a block that is output as written, which are `raw` and
    ///   `endraw` in [Jinja][Jinja `raw`], [Liquid][Liquid `raw`] and
    ///   [Nunjucks][Nunjucks `raw`], and `verbatim` and `endverbatim` in
    ///   [Django][Django `verbatim`] and [Twig][Twig `verbatim`].
    ///
    /// [Liquid `elsif`]: https://shopify.github.io/liquid/tags/control-flow/#elsif--else
    /// [Django `if`]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#if
    /// [Jinja `if`]: https://jinja.palletsprojects.com/en/stable/templates/#if
    /// [Nunjucks `if`]: https://mozilla.github.io/nunjucks/templating.html#if
    /// [Twig `if`]: https://twig.symfony.com/doc/3.x/tags/if.html
    /// [Jinja `raw`]: https://jinja.palletsprojects.com/en/stable/templates/#escaping
    /// [Liquid `raw`]: https://shopify.github.io/liquid/tags/template/#raw
    /// [Nunjucks `raw`]: https://mozilla.github.io/nunjucks/templating.html#raw
    /// [Django `verbatim`]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#verbatim
    /// [Twig `verbatim`]: https://twig.symfony.com/doc/3.x/tags/verbatim.html
    fn warn_foreign_tag(&mut self, name: Token) {
        let foreign = Syntax::tag(self._source.hunk(name.span))
            .and_then(|syntax| self.options.dialect.foreign(syntax));

        if let Some(foreign) = foreign {
            let warning =
                ParseWarning::new(ParseWarningKind::ForeignTag(foreign), self.make_span(name.span));
            self.add_warning(warning);
        }
    }

    /// Skip the rest of the tag after its name `tag`, which is `else` or an end
    /// tag, and return the range of the rest, if there is any. Liquid ignores
    /// it: its [`if`] and [`for`] after `else`, and a [block] or a [`raw`],
    /// [`comment`] or [`doc`] block after its end tag's name. It is likely a
    /// mistake, e.g. `{% else if b %}`, which is an `else` whatever `b` is,
    /// so it is warned about. `in_if` says whether the tag is the `else` of an
    /// `if` block, which could be an `elsif` clause instead.
    ///
    /// [`if`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/if.rb#L73-L78
    /// [`for`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/for.rb#L56-L59
    /// [block]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/block.rb#L84
    /// [`raw`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/raw.rb#L27
    /// [`comment`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/comment.rb#L53-L54
    /// [`doc`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/doc.rb#L42-L46
    fn parse_ignored_markup(&mut self, tag: Keyword, in_if: bool) -> Option<ByteRange> {
        let dialect = self.options.dialect;
        if !dialect.else_and_end_tags_ignore_markup() || self.exhausted() {
            return None;
        }

        let start = self.current_pos();
        while self.has_token() {
            self.skip_token(); // `<token>` Skip each token of the rest.
        }
        let range = start.join(self.previous_pos());

        let markup = self._source.hunk(range).into();
        let kind = ParseWarningKind::IgnoredMarkup { dialect, tag, markup, in_if };
        self.add_warning(ParseWarning::new(kind, self.make_span(range)));

        Some(range)
    }

    /// Create an error at the current location.
    pub(crate) fn err_with_location<T>(
        &self,
        kind: ParseErrorKind,
        expected: ExpectedItem,
        received: Option<TokenKind>,
        span: ByteRange,
    ) -> ParseResult<T> {
        Err(self.make_err(kind, expected, received, Some(span)))
    }

    /// Generate an error that represents that within the current [AstGen] the
    /// `end of file` state should be reached. This means that either in the
    /// root generator there are no more tokens or within a nested generator
    /// (such as if the generator is within a brackets) that it should now
    /// read no more tokens.
    pub(crate) fn expected_eof<T>(&self) -> ParseResult<T> {
        let tok = self.peek().unwrap_or_else(|| self.previous_token());

        self.err_with_location(
            ParseErrorKind::UnExpected,
            ExpectedItem::empty(),
            Some(tok.kind),
            tok.span,
        )
    }

    #[inline]
    pub(crate) fn make_unexpected_eof(&self) -> ParseError {
        self.make_err(ParseErrorKind::UnExpected, ExpectedItem::empty(), None, None)
    }

    /// Create new AST generator from a provided token stream with inherited
    /// module resolver and a provided parent span.
    fn new_frame<T>(
        &mut self,
        start: usize,
        len: usize,
        parent_span: ByteRange,
        delimiter: Delimiter,
        mut g: impl FnMut(&mut Self) -> T,
    ) -> T {
        let mut new_frame =
            ParseFrame::from_stream(&self.frame.stream()[start..(start + len)], parent_span);
        new_frame.delimiter = Some(delimiter);
        let old_frame = std::mem::replace(&mut self.frame, new_frame);
        let result = g(self);

        // Ensure that the generator token stream has been exhausted
        if !self.error.get() && self.has_token() {
            self.maybe_add_error::<()>(self.expected_eof());
        }

        // Now finally swap back the old frame
        let _ = std::mem::replace(&mut self.frame, old_frame);
        result
    }

    /// Function to peek ahead and match some parsing function that returns a
    /// [Option<T>]. If The result is an error, the function wil reset the
    /// current offset of the token stream to where it was the function was
    /// peeked. This is essentially a convertor from a [ParseResult<T>]
    /// into an [Option<T>] with the side effect of resetting the parser state
    /// back to it's original settings.
    pub(crate) fn peek_resultant_fn<T, E>(
        &mut self,
        mut parse_fn: impl FnMut(&mut Self) -> Result<T, E>,
    ) -> Option<T> {
        let start = self.position();

        match parse_fn(self) {
            Ok(result) => Some(result),
            Err(_) => {
                self.set_pos(start);
                None
            }
        }
    }
    /// Record the [ByteRange] that a parse function `f` traversed during
    /// its execution. This is useful for tracking the span of a node that
    /// is generated from a parse function.
    #[inline]
    pub(crate) fn track_span<T, E>(
        &mut self,
        mut f: impl FnMut(&mut Self) -> Result<T, E>,
    ) -> Result<(T, ByteRange), E> {
        let start = self.current_pos();
        let result = f(self)?;
        let end = self.previous_pos();

        Ok((result, start.join(end)))
    }

    /// Function to parse the next [Token] with the specified [TokenKind].
    ///
    /// ##Note: Don't use `parse_token()` to parse a tree token.
    pub(crate) fn parse_token(&self, atom: TokenKind) -> ParseResult<()> {
        debug_assert!(!atom.is_tree());

        match self.peek() {
            Some(token) if token.kind == atom => {
                self.skip_fast(token.kind); // non-tree
                Ok(())
            }
            token => self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::from(atom),
                token.map(|t| t.kind),
                token.map_or_else(|| self.eof_pos(), |t| t.span),
            ),
        }
    }

    /// Utility function to parse a brace tree as the next token, if a brace
    /// tree isn't present, then an error is generated.
    pub(crate) fn in_tree<T>(
        &mut self,
        delimiter: Delimiter,
        error: Option<ParseErrorKind>,
        g: impl FnMut(&mut Self) -> ParseResult<T>,
    ) -> ParseResult<T> {
        match self.peek() {
            // A line of a `liquid` tag is a tag, like a `{% %}` tree.
            Some(Token { kind: TokenKind::Tree(inner, len), span })
                if *inner == delimiter || (inner.is_tag() && delimiter.is_tag()) =>
            {
                // The start of the tree is the actual `Tree` token, and then we slice
                // from it up to the specified `len` of the tree.
                let start = self.position() + 1;

                self.skip_token(); // We want to update our position, when we return to this generator.
                self.new_frame(start, *len as usize, *span, *inner, g)
            }
            token => self.err_with_location(
                error.unwrap_or(ParseErrorKind::UnExpected),
                ExpectedItem::from(delimiter),
                token.map(|tok| tok.kind),
                token.map_or_else(|| self.current_pos(), |tok| tok.span),
            ),
        }
    }

    /// The whitespace control markers of the tag that the current frame is in,
    /// e.g. the `-`s in `{%- if x -%}`. The lexer leaves the markers out of
    /// the tag's tokens, so they are read from its source, see [tag_trim]. A
    /// line of a `liquid` tag has no delimiters, and so no markers. See
    /// [Jinja], [Liquid], [Nunjucks] and [Twig].
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#whitespace-control
    /// [Liquid]: https://shopify.github.io/liquid/basics/whitespace/
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#whitespace-control
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#whitespace-control
    pub(crate) fn trim(&self) -> ast::TrimMarker {
        if self.frame.delimiter == Some(Delimiter::Line) {
            return ast::TrimMarker::default();
        }

        tag_trim(self._source.hunk(self.range()), self.options.dialect.trim_markers())
    }

    /// Run `g` in the tag that is the next token, like [Parser::in_tree], and
    /// also return the tag's whitespace control markers.
    pub(crate) fn in_tag<T>(
        &mut self,
        delimiter: Delimiter,
        mut g: impl FnMut(&mut Self) -> ParseResult<T>,
    ) -> ParseResult<(T, ast::TrimMarker)> {
        self.in_tree(delimiter, None, |this| Ok((g(this)?, this.trim())))
    }

    /// Parse the rest of the tag that the current frame is in as its markup,
    /// with `g`, e.g. the expression in a `{{ }}`, or the condition of an
    /// `if`. If the markup doesn't parse in a dialect whose markup can be lax,
    /// it is kept as it is written instead, as an [ast::Expr::Unparsed] that
    /// `lax` makes into what `g` would return, and the error that parsing it
    /// reported is a warning.
    fn parse_markup<T>(
        &mut self,
        g: impl FnOnce(&mut Self) -> ParseResult<T>,
        lax: impl FnOnce(&Self, AstNode<'a, ast::Expr<'a>>) -> T,
    ) -> ParseResult<T> {
        // [Liquid] parses markup strictly, and if that fails, records a warning
        // and parses it again with its lax parser, which takes nearly any
        // markup that isn't empty.
        //
        // [Liquid]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/parser_switching.rb#L38-L44
        if !self.options.dialect.markup_can_be_lax() || self.exhausted() {
            return g(self);
        }

        let (position, start) = (self.position(), self.current_pos());
        let (errors, frame_error) = (self.diagnostics.errors.len(), self.frame.error.get());
        let result = g(self)
            .and_then(|value| if self.exhausted() { Ok(value) } else { self.expected_eof() });

        // A tree in the markup reports an error at what is left in it, e.g. the
        // `c` in `a[b c]`, without stopping the parse, so that error is first.
        let error = match (self.diagnostics.errors.get(errors).copied(), result) {
            (None, Ok(value)) => return Ok(value),
            (Some(error), _) | (None, Err(error)) => error,
        };
        self.diagnostics.errors.truncate(errors);
        self.frame.error.set(frame_error);
        self.add_warning(ParseWarning::new(ParseWarningKind::LaxMarkup(error), error.span()));

        // Skip to the end of the tag from where the markup starts, a token or a
        // whole tree at a time, so that the markup's span ends with the last.
        self.set_pos(position);
        while self.has_token() {
            self.skip_token();
        }

        let unparsed = ast::Expr::Unparsed(ast::UnparsedExpr {});
        let expr = self.node_with_joined_span(unparsed, start);
        Ok(lax(self, expr))
    }

    /// Get the current [TagContext] that the parser is in, if any.
    pub(crate) fn tag_context(&self) -> Option<TagContext> {
        self.frame.tag_context
    }

    /// Run a function with a specified tag context.
    pub(crate) fn with_tag_context<T>(
        &mut self,
        ctx: TagContext,
        g: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let old_ctx = self.frame.tag_context;
        self.frame.tag_context = Some(ctx);
        let result = g(self);
        self.frame.tag_context = old_ctx;
        result
    }

    /// Function to parse a token atom optionally. If the appropriate token atom
    /// is present we advance the token count, if not then just return [None].
    ///
    /// ##Note: Don't use `parse_token_fast()` to parse a tree token.
    pub(crate) fn parse_token_fast(&self, kind: TokenKind) -> Option<()> {
        debug_assert!(!kind.is_tree());

        match self.peek() {
            Some(token) if token.kind == kind => {
                self.skip_fast(kind); // token, non-tree
                Some(())
            }
            _ => None,
        }
    }

    pub fn parse_document(&mut self) -> AstNode<'a, ast::Document<'a>> {
        // if self.options.recovery {
        //     log::info!("recovery mode enabled");
        // }

        let start = self.current_pos();
        let mut children = self.arena.vec();

        while self.peek().is_some() {
            match self.parse_statement() {
                Ok(Some(node)) => children.push(node),
                Ok(None) => panic!("unexpected None value"),
                Err(err) => {
                    // Parsing the statement failed, we proceed onwards.
                    self.skip_token();
                    self.add_error(err)
                }
            }
        }

        let contents = self.nodes_with_joined_span(children, start);
        let document = self.node_with_joined_span(ast::Body { contents }, start);
        self.node_with_joined_span(ast::Document { document }, start)
    }

    fn parse_statement(&mut self) -> ParseResult<Option<AstNode<'a, ast::Statement<'a>>>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        let statement = match token.kind {
            // For parsing text nodes.
            TokenKind::Text => {
                self.skip_fast(TokenKind::Text); // `<text>` Skip the text token.
                self.warn_hash_comments(token.span);
                Ok(self.node_with_span(ast::Statement::text(), token.span))
            }
            // For parsing `{% ... %}` blocks.
            TokenKind::Tree(delimiter, _) if delimiter.is_tag() => return self.parse_tag(),
            // For parsing `{{ ... }}` blocks.
            TokenKind::Tree(Delimiter::Brace, _) => self.parse_variable_block(),
            TokenKind::Comment => self.parse_comment(),
            _ => self.err_with_location(
                ParseErrorKind::Statement,
                ExpectedItem::empty(),
                None,
                self.expected_pos(),
            ),
        }?;

        Ok(Some(statement))
    }

    fn parse_comment(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        self.skip_fast(TokenKind::Comment); // `<comment>` Skip the comment token.

        Ok(self.node_with_joined_span(ast::Statement::Comment(ast::Comment {}), token.span))
    }

    fn parse_tag(&mut self) -> ParseResult<Option<AstNode<'a, ast::Statement<'a>>>> {
        // The last token must be a percent tree, but we ensure this since
        // we pass the responsibility of consuming the header token to then child
        // functions.
        debug_assert!(self.peek().is_some_and(|tok| tok.kind.is_tag_tree()));

        let token = self.peek_raw(1).copied().ok_or_else(|| self.make_unexpected_eof())?;
        if matches!(token.kind, TokenKind::Ident | TokenKind::Keyword(Keyword::Raw)) {
            self.warn_foreign_tag(token);
        }

        let statement = match token.kind {
            TokenKind::Ident => self.in_tree(Delimiter::Percent, None, |g| {
                let name = g.parse_name()?;
                let args = g.parse_args()?;

                if g.parse_token_fast(TokenKind::Keyword(Keyword::As)).is_some() {
                    let subject = g.node_with_span(
                        ast::Expr::Var(VarExpr {
                            name: ast::Name::new(ast::Identifier::from(0u32)),
                        }),
                        token.span,
                    );
                    let value = g.node_with_joined_span(
                        ast::Expr::Call(ast::CallExpr { subject, args }),
                        token.span,
                    );

                    let name = g.parse_name()?;

                    // A custom tag can take more after the name, e.g. Django's
                    // `{% get_admin_log 10 as log for_user user %}`, which is
                    // kept as it is written.
                    if !g.exhausted() {
                        g.exhaust();
                        return Ok(g.node_with_joined_span(
                            ast::Statement::Tag(ast::Tag::Unprocessable(ast::UnprocessableTag {})),
                            g.current_pos(),
                        ));
                    }

                    Ok(g.node_with_joined_span(
                        ast::Statement::Tag(ast::Tag::Assignment(ast::Assignment { name, value })),
                        g.range(),
                    ))
                } else if !g.exhausted() {
                    g.exhaust();
                    Ok(g.node_with_joined_span(
                        ast::Statement::Tag(ast::Tag::Unprocessable(ast::UnprocessableTag {})),
                        g.current_pos(),
                    ))
                } else {
                    Ok(g.node_with_joined_span(
                        ast::Statement::Tag(ast::Tag::Generic(ast::GenericTag { name, args })),
                        g.range(),
                    ))
                }
            }),

            TokenKind::Keyword(keyword) => match keyword {
                // Key control flow components, once that imply a more complex structure
                // of subsequent tags.
                Keyword::For => {
                    self.with_tag_context(TagContext::For, |g| g.parse_for_loop(ast::LoopKind::For))
                }
                Keyword::TableRow => self.with_tag_context(TagContext::For, |g| {
                    g.parse_for_loop(ast::LoopKind::TableRow)
                }),

                Keyword::If | Keyword::Unless => {
                    self.with_tag_context(TagContext::If, |g| g.parse_if_block())
                }
                Keyword::Case => self.with_tag_context(TagContext::Case, |g| g.parse_case_block()),
                Keyword::Guard => {
                    self.with_tag_context(TagContext::Guard, |g| g.parse_guard_block())
                }
                Keyword::Capture => {
                    self.with_tag_context(TagContext::Capture, |g| g.parse_capture_block())
                }
                Keyword::Render => self.parse_render_statement(),
                Keyword::Liquid => self.parse_liquid_tag(),

                // Control flow tags, that are effectively standalone.
                Keyword::Break => self.parse_break_statement(),
                Keyword::Continue => self.parse_continue_statement(),

                // Block tags, which have a structure that they must be terminated with an
                // end tag, e.self. `{% block %} ... {% endblock %}`.
                Keyword::With => self.with_tag_context(TagContext::With, |g| g.parse_with_block()),
                Keyword::Block => {
                    self.with_tag_context(TagContext::Block, |g| g.parse_block_statement())
                }
                Keyword::Raw => self.with_tag_context(TagContext::Raw, |g| g.parse_raw_block()),
                Keyword::Comment => self.with_tag_context(TagContext::Comment, |g| {
                    g.parse_comment_block(Keyword::EndComment)
                }),
                // The lexer reads the contents of a Liquid `{% doc %}` block as
                // text, so it is a comment that ends at `{% enddoc %}`.
                Keyword::Doc => self.parse_comment_block(Keyword::EndDoc),

                kwd if let Some(ctx) = self.tag_context()
                    && ctx.applies_to(kwd) =>
                {
                    return Ok(None);
                }

                // Effectively, special functions that we keep track of.
                Keyword::Extends => self.parse_extends_statement(),
                Keyword::Include | Keyword::IncludeRelative => self.parse_include_statement(),
                Keyword::Load => self.parse_load_statement(),
                Keyword::Import => self.parse_import_statement(),
                _ => {
                    // The body of an empty loop starts at `{% empty %}` in
                    // [Django], and at `{% else %}` in [Jinja], [Liquid],
                    // [Nunjucks] and [Twig], so the other's tag in a loop is
                    // likely written in another dialect.
                    //
                    // [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
                    // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
                    // [Liquid]: https://shopify.github.io/liquid/tags/iteration/#else
                    // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#for
                    // [Twig]: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
                    if matches!(keyword, Keyword::Else | Keyword::Empty)
                        && self.tag_context() == Some(TagContext::For)
                        && let Some(syntax) = Syntax::tag(keyword.as_ref())
                    {
                        self.mark_foreign(token.span, syntax);
                    }

                    self.err_with_location(
                        ParseErrorKind::Tag,
                        ExpectedItem::Ident,
                        Some(token.kind),
                        token.span,
                    )
                }
            },
            _ => self.err_with_location(
                ParseErrorKind::Tag,
                ExpectedItem::Ident,
                Some(token.kind),
                token.span,
            ),
        }?;

        Ok(Some(statement))
    }

    fn parse_variable_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        let (expr, trim) = self.in_tag(Delimiter::Brace, |g| {
            g.parse_markup(Self::parse_variable_expr, |_, expr| expr)
        })?;

        Ok(self
            .node_with_joined_span(ast::Statement::Inline(ast::Inline { expr, trim }), token.span))
    }

    /// Parse the expression in a `{{ }}`.
    fn parse_variable_expr(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        // Jinja's [`Parser.subparse`], Nunjucks' [`Parser.parseNodes`] and
        // Twig's [`Parser::subparse`] parse `{{ }}` as any expression, rather
        // than a single value.
        //
        // [`Parser.subparse`]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L1019-L1022
        // [`Parser.parseNodes`]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L1317-L1318
        // [`Parser::subparse`]: https://github.com/twigphp/Twig/blob/v3.30.0/src/Parser.php#L187-L190
        let (mut subject, subject_span) = self.track_span(|g| {
            if g.options.dialect.variable_supports_operators() {
                g.parse_compound_expr(0)
            } else {
                g.parse_expr()
            }
        })?;

        // Items separated by commas are a tuple without its parentheses in
        // [Jinja], e.g. `{{ a, b }}`.
        //
        // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L1019-L1022
        if self.options.dialect.variables_take_bare_tuples()
            && self.peek_kind() == Some(TokenKind::Comma)
        {
            let mut items = self.arena.vec();
            items.push(subject);
            while self.parse_token_fast(TokenKind::Comma).is_some() && !self.exhausted() {
                items.push(self.parse_compound_expr(0)?);
            }

            let children = self.nodes_with_joined_span(items, subject_span);
            subject = self
                .node_with_joined_span(ast::Expr::Tuple(ast::TupleExpr { children }), subject_span);
        }

        // A name that is followed by anything other than an operator, e.g.
        // `{{ x "a" b=c }}`, is called with the rest as its arguments. A name
        // in parentheses, e.g. `(x)`, is a name in the tree too, but its span
        // leaves out the parentheses that `subject_span` covers, and it isn't
        // called.
        if !self.exhausted() && subject.body.is_var() && subject.range() == subject_span {
            // This might be a call expression, try and parse the arguments
            let args = self.parse_args()?;

            return Ok(self.node_with_joined_span(
                ast::Expr::Call(ast::CallExpr { subject, args }),
                subject_span,
            ));
        }

        Ok(subject)
    }

    /// Parse an expression with the binary operators that bind at least as
    /// tightly as `min_precedence`, see [ast::Dialect::infix_binding_power].
    /// At the lowest precedence, it can also be a conditional expression, see
    /// [Parser::parse_conditional], which binds looser than any operator.
    fn parse_compound_expr(
        &mut self,
        min_precedence: u8,
    ) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        // first of all, we want to get the lhs...
        let (mut lhs, lhs_span) = self.track_span(|this| this.parse_expr())?;

        loop {
            let op_start = self.current_pos();
            // this doesn't consider operators that have an 'eq' variant because that is
            // handled at the statement level, since it isn't really a binary
            // operator...
            let (Some(op), consumed_tokens) = self.parse_bin_op() else {
                if min_precedence == ast::Dialect::CONDITIONAL_BINDING_POWER
                    && self.at_conditional()
                {
                    lhs = self.parse_conditional(lhs, lhs_span)?;
                    continue;
                }

                break;
            };

            // An operator that the dialect doesn't have, e.g. `*` in Django,
            // doesn't continue the expression. It is left for the tag, which
            // reports it as unexpected, like any token after an expression.
            // How tightly each operator binds follows [Django], [Jinja],
            // [Liquid], [Nunjucks] and [Twig].
            //
            // [Django]: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
            // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
            // [Liquid]: https://shopify.github.io/liquid/basics/operators/#order-of-operations
            // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L776-L992
            // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
            let Some((l_precedence, r_precedence)) = self.options.dialect.infix_binding_power(op)
            else {
                break;
            };

            // check if we have higher precedence than the lhs expression...
            if l_precedence < min_precedence {
                break;
            }

            // Now skip the consumed tokens...
            self.skip(consumed_tokens);

            let op_span = op_start.join(self.current_pos());
            let rhs = match op {
                ast::BinOp::Is | ast::BinOp::IsNot if self.at_two_word_test() => {
                    self.parse_two_word_test()?
                }
                ast::BinOp::Is | ast::BinOp::IsNot
                    if self.options.dialect.test_arg_needs_no_parentheses() =>
                {
                    self.parse_test()?
                }
                _ => self.parse_compound_expr(r_precedence)?,
            };

            //v transform the operator into an `BinaryExpr`
            let op = self.node_with_span(op, op_span);
            lhs =
                self.node_with_joined_span(ast::Expr::Bin(ast::BinExpr { lhs, rhs, op }), lhs_span);
        }

        Ok(lhs)
    }

    /// Whether a conditional expression continues the expression before it,
    /// see [Parser::parse_conditional].
    fn at_conditional(&self) -> bool {
        match self.peek_kind() {
            Some(TokenKind::Keyword(Keyword::If)) => self.options.dialect.has_inline_if(),
            Some(TokenKind::Question) => self.options.dialect.has_conditional_operators(),
            _ => false,
        }
    }

    /// Parse the rest of a conditional expression after `expr`, at `span`:
    /// `then if condition else otherwise` in [Jinja] and [Nunjucks], whose
    /// `else` part can be left out, and `condition ? then : otherwise` in
    /// [Twig], whose `:` part can be left out. Each part can be a conditional
    /// expression too, apart from Jinja's condition, so
    /// `a if b else c if d else e` is `a if b else (c if d else e)`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L521-L534
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L758-L774
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/ConditionalTernaryExpressionParser.php#L29-L41
    fn parse_conditional(
        &mut self,
        expr: AstNode<'a, ast::Expr<'a>>,
        span: ByteRange,
    ) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let dialect = self.options.dialect;

        let (condition, then) = if self.parse_token_fast(TokenKind::Keyword(Keyword::If)).is_some()
        {
            (self.parse_compound_expr(ast::Dialect::CONDITIONAL_BINDING_POWER + 1)?, expr)
        } else {
            self.skip_fast(TokenKind::Question); // `?` Skip the operator.
            (expr, self.parse_compound_expr(0)?)
        };

        let otherwise = match self.peek_kind() {
            Some(TokenKind::Keyword(Keyword::Else)) if dialect.has_inline_if() => {
                self.skip_fast(TokenKind::Keyword(Keyword::Else)); // `else` Skip the keyword.
                Some(self.parse_compound_expr(0)?)
            }
            Some(TokenKind::Colon) if dialect.has_conditional_operators() => {
                self.skip_fast(TokenKind::Colon); // `:` Skip the separator.
                Some(self.parse_compound_expr(0)?)
            }
            _ => None,
        };

        Ok(self.node_with_joined_span(
            ast::Expr::Cond(ast::CondExpr { condition, then, otherwise }),
            span,
        ))
    }

    fn parse_expr(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let start = self.current_pos();
        let mut expr = self.parse_value()?;

        // A filter applies to everything before it, and its result can be
        // accessed further, e.g. `users|first.name`.
        while self.peek().is_some_and(|token| token.kind == TokenKind::Pipe) {
            let subject_span = start.join(self.previous_pos());
            self.skip_fast(TokenKind::Pipe); // `<pipe>` Skip the pipe token.
            let filter = self.parse_filter(subject_span)?;
            let subject = self.node_with_joined_span(
                ast::Expr::FilteredExpr(ast::FilteredExpr { subject: expr, filter }),
                subject_span,
            );
            expr = self.parse_singular_expr(subject, subject_span)?;
        }

        Ok(expr)
    }

    fn parse_expr_component(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        // ##Note: Each child path is responsible for skipping the current `token`.
        Ok(match token.kind {
            kind if kind.is_unary_op() => return self.parse_unary_expr(token),

            _ if let Some(lit) = self.liquid_literal(token) => {
                self.skip_fast(token.kind); // `<lit>` Skip the literal token.
                self.node_with_span(ast::Expr::Lit(ast::LitExpr { lit }), token.span)
            }
            // An arrow function in [Twig], e.g. `x => x.name`, or
            // `(a, b) => a + b`.
            //
            // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/ArrowExpressionParser.php#L28-L32
            TokenKind::Ident | TokenKind::Tree(Delimiter::Paren, _)
                if self.options.dialect.has_arrow_functions()
                    && matches!(self.peek_second(), Some(tok!(Arrow))) =>
            {
                return self.parse_arrow_function(token);
            }
            // A [Liquid] range, e.g. `(1..n)`.
            //
            // [Liquid]: https://shopify.github.io/liquid/tags/iteration/#range
            TokenKind::Tree(Delimiter::Paren, _) if self.options.dialect.has_ranges() => {
                return self.parse_range(token);
            }
            // Parentheses group an expression in [Jinja], [Nunjucks] and
            // [Twig], e.g. `(a or b)`, and hold a tuple in Jinja.
            //
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#logic
            // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#logic
            // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#logic
            TokenKind::Tree(Delimiter::Paren, _) if self.options.dialect.has_grouping() => {
                return self.parse_parenthesised_expr(token);
            }
            // A list in [Jinja], an array in [Nunjucks], or a sequence in
            // [Twig], e.g. `['a', 'b']`.
            //
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
            // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
            // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
            TokenKind::Tree(Delimiter::Bracket, _) if self.options.dialect.has_lists() => {
                return self.parse_list_literal(token);
            }
            // A mapping in [Jinja] and [Nunjucks], where it is a dict, and
            // [Twig], e.g. `{'a': 1}`.
            //
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
            // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
            // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
            TokenKind::Tree(Delimiter::Curly, _) if self.options.dialect.has_mappings() => {
                return self.parse_mapping(token);
            }
            // A string that [Django] translates, e.g. `_("Name")`, which is a
            // string rather than a call.
            //
            // [Django]: https://docs.djangoproject.com/en/stable/topics/i18n/translation/#string-literals-passed-to-tags-and-filters
            TokenKind::Ident if self.at_translated_string(token) => {
                self.skip_fast(TokenKind::Ident); // `_` Skip the underscore.
                self.skip_token(); // `(...)` Skip the parentheses, and the string in them.
                let lit = ast::Lit::Str(ast::StrLit {});
                self.node_with_joined_span(ast::Expr::Lit(ast::LitExpr { lit }), token.span)
            }

            kw @ TokenKind::Keyword(keyword)
                if keyword.identifier_like() || self.keyword_is_name(keyword) =>
            {
                self.skip_fast(kw); // `<kw>` Skip the identifier token.
                self.node_with_joined_span(
                    ast::Expr::Var(ast::VarExpr {
                        name: ast::Name::new(ast::Identifier::from(0u32)),
                    }),
                    token.span,
                )
            }
            TokenKind::Ident => {
                self.skip_fast(TokenKind::Ident); // `<ident>` Skip the identifier token.
                self.node_with_joined_span(
                    ast::Expr::Var(ast::VarExpr {
                        name: ast::Name::new(ast::Identifier::from(0u32)),
                    }),
                    token.span,
                )
            }
            kind if kind.is_lit() => self.node_with_joined_span(
                ast::Expr::Lit(ast::LitExpr { lit: self.parse_lit()? }),
                token.span,
            ),
            _ => {
                return self.err_with_location(
                    ParseErrorKind::UnExpected,
                    ExpectedItem::Expr,
                    Some(token.kind),
                    token.span,
                );
            }
        })
    }

    /// Whether `keyword` is a name in an expression, as any keyword is in
    /// [Jinja] and [Twig] apart from those that their expressions have: `and`,
    /// `or`, `not`, `in` and `is`, Jinja's `if` and `else`, and `True` and
    /// `False`.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L648-L661
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Prefix/LiteralExpressionParser.php#L37-L56
    fn keyword_is_name(&self, keyword: Keyword) -> bool {
        self.options.dialect.keywords_are_names()
            && !matches!(
                keyword,
                Keyword::And
                    | Keyword::Or
                    | Keyword::Not
                    | Keyword::In
                    | Keyword::Is
                    | Keyword::If
                    | Keyword::Else
                    | Keyword::True
                    | Keyword::False
            )
    }

    /// The Liquid literal that `token` is, if any, e.g. [`nil`] or [`empty`].
    /// In the other dialects, these are names like any other.
    ///
    /// [`nil`]: https://shopify.github.io/liquid/basics/types/#nil
    /// [`empty`]: https://shopify.github.io/liquid/basics/types/#emptydrop
    fn liquid_literal(&self, token: Token) -> Option<ast::Lit> {
        if !self.options.dialect.has_nil_and_empty_literals() {
            return None;
        }

        match (token.kind, self._source.hunk(token.span)) {
            (TokenKind::Ident, "true") => Some(ast::Lit::Bool(ast::BoolLit { value: true })),
            (TokenKind::Ident, "false") => Some(ast::Lit::Bool(ast::BoolLit { value: false })),
            (TokenKind::Ident, "nil" | "null") => Some(ast::Lit::Nil(ast::NilLit {})),
            (TokenKind::Ident, "blank") => {
                Some(ast::Lit::Empty(ast::EmptyLit { kind: ast::EmptyKind::Blank }))
            }
            (TokenKind::Keyword(Keyword::Empty), _) => {
                Some(ast::Lit::Empty(ast::EmptyLit { kind: ast::EmptyKind::Empty }))
            }
            _ => None,
        }
    }

    /// Parse a Liquid range of integers, i.e. `(1..n)`. Its bounds are values,
    /// which have no filters.
    fn parse_range(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let (start, end) = self.in_tree(Delimiter::Paren, None, |g| {
            let start = g.parse_value()?;
            g.parse_token(TokenKind::DotDot)?;
            Ok((start, g.parse_value()?))
        })?;

        Ok(self.node_with_span(ast::Expr::Range(ast::RangeExpr { start, end }), token.span))
    }

    /// Parse a subscript in [Jinja] and [Twig], which is an expression, e.g.
    /// `i + 1` in `items[i + 1]`, or a slice, e.g. `1:` in `items[1:]`, whose
    /// parts can be left out, and which can have a step in Jinja.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L840-L870
    /// [Twig]: https://twig.symfony.com/doc/3.x/filters/slice.html
    fn parse_subscript(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let span = self.current_pos();
        let start = self.parse_slice_part()?;
        if self.parse_token_fast(TokenKind::Colon).is_none() {
            // Without a `:`, the subscript is an expression, which an empty
            // subscript, e.g. `items[]`, is missing.
            return match start {
                Some(expr) => Ok(expr),
                None => self.parse_compound_expr(0),
            };
        }

        let stop = self.parse_slice_part()?;
        let step = match self.parse_token_fast(TokenKind::Colon) {
            Some(()) => self.parse_slice_part()?,
            None => None,
        };

        Ok(self.node_with_joined_span(ast::Expr::Slice(ast::SliceExpr { start, stop, step }), span))
    }

    /// Parse a part of a slice, see [Self::parse_subscript], which is left out
    /// when a `:` or the end of the subscript is next.
    fn parse_slice_part(&mut self) -> ParseResult<Option<AstNode<'a, ast::Expr<'a>>>> {
        match self.peek_kind() {
            None | Some(TokenKind::Colon) => Ok(None),
            Some(_) => Ok(Some(self.parse_compound_expr(0)?)),
        }
    }

    /// Parse an arrow function, e.g. `x => x.name`, or `(a, b) => a + b`
    /// with its parameters in parentheses.
    fn parse_arrow_function(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let params = if token.kind == TokenKind::Ident {
            let mut params = self.arena.vec();
            params.push(self.parse_name()?);
            self.nodes_with_span(params, token.span)
        } else {
            self.in_tree(Delimiter::Paren, None, |g| {
                let start = g.current_pos();
                let mut params = g.arena.vec();

                while !g.exhausted() {
                    params.push(g.parse_name()?);

                    if g.parse_token_fast(TokenKind::Comma).is_none() {
                        break;
                    }
                }

                Ok(g.nodes_with_joined_span(params, start))
            })?
        };

        self.parse_token(TokenKind::Arrow)?;
        let expr = self.parse_compound_expr(0)?;
        Ok(self
            .node_with_joined_span(ast::Expr::Arrow(ast::ArrowExpr { params, expr }), token.span))
    }

    /// Whether the next tokens are the name of a test that is two words, as
    /// Twig's [`same as`] and [`divisible by`] are.
    ///
    /// [`same as`]: https://twig.symfony.com/doc/3.x/tests/sameas.html
    /// [`divisible by`]: https://twig.symfony.com/doc/3.x/tests/divisibleby.html
    fn at_two_word_test(&self) -> bool {
        let (Some(first), Some(second)) = (self.peek(), self.peek_second()) else {
            return false;
        };

        self.options.dialect.has_two_word_tests()
            && first.kind == TokenKind::Ident
            && matches!(
                (self._source.hunk(first.span), self._source.hunk(second.span)),
                ("same", "as") | ("divisible", "by")
            )
    }

    /// Parse a test whose name is two words, see [Self::at_two_word_test],
    /// with its argument, which [Twig] takes in parentheses, e.g.
    /// `same as(false)`, or else after it, e.g. `same as false`.
    ///
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/IsExpressionParser.php#L43-L47
    fn parse_two_word_test(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let start = self.current_pos();
        self.skip_token(); // `same` | `divisible` Skip the first word.
        self.skip_token(); // `as` | `by` Skip the second word.

        let name = ast::Name::new(ast::Identifier::from(0u32));
        let test = self.node_with_joined_span(ast::Expr::Var(ast::VarExpr { name }), start);
        if matches!(self.peek_kind(), Some(TokenKind::Tree(Delimiter::Paren, _))) {
            return self.parse_singular_expr(test, start.join(self.previous_pos()));
        }

        // A test binds as tightly as `is`, so its argument takes the operators
        // that bind tighter.
        let arg_start = self.current_pos();
        let value = self.parse_compound_expr(
            self.options.dialect.infix_binding_power(ast::BinOp::Is).map_or(0, |(_, right)| right),
        )?;
        let mut args = self.arena.vec();
        args.push(self.node_with_joined_span(
            ast::Arg { name: None, value: Some(value), unpack: None },
            arg_start,
        ));
        let args = self.nodes_with_joined_span(args, arg_start);

        Ok(self
            .node_with_joined_span(ast::Expr::Call(ast::CallExpr { subject: test, args }), start))
    }

    /// Parse the test after `is`, e.g. `defined` or `divisibleby(3)`, whose
    /// argument [Jinja] also takes after it without parentheses, as a single
    /// value, e.g. `divisibleby 3`, which is a call of the test too.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L971-L986
    fn parse_test(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let start = self.current_pos();
        let test = self.parse_value()?;

        let arg = match self.peek_kind() {
            Some(
                TokenKind::Ident
                | TokenKind::Str
                | TokenKind::Number(_)
                | TokenKind::Keyword(Keyword::True | Keyword::False)
                | TokenKind::Tree(Delimiter::Paren | Delimiter::Bracket | Delimiter::Curly, _),
            ) => self.parse_value()?,
            _ => return Ok(test),
        };

        let mut args = self.arena.vec();
        let arg_span = arg.range();
        args.push(
            self.node_with_span(ast::Arg { name: None, value: Some(arg), unpack: None }, arg_span),
        );
        let args = self.nodes_with_span(args, arg_span);

        Ok(self
            .node_with_joined_span(ast::Expr::Call(ast::CallExpr { subject: test, args }), start))
    }

    /// Parse an expression in parentheses, e.g. `(a or b)`, which groups it.
    /// The tree has no node for the parentheses, see [ast::Expr], so this is
    /// the expression in them. In [Jinja], items separated by commas are a
    /// tuple instead, e.g. `('a', 'b')`, as are `()` and a single item with a
    /// comma after it. In [Nunjucks], they are a group of several expressions,
    /// e.g. `(rows.unshift(row), rows)`, which is a tuple here too.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L685-L752
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L1147-L1176
    fn parse_parenthesised_expr(
        &mut self,
        token: Token,
    ) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let tuples = self.options.dialect.has_tuples() || self.options.dialect.has_comma_groups();

        self.in_tree(Delimiter::Paren, None, |g| {
            let start = g.current_pos();
            let mut items = g.arena.vec();

            if !(tuples && g.exhausted()) {
                let expr = g.parse_compound_expr(0)?;
                if !(tuples && g.peek_kind() == Some(TokenKind::Comma)) {
                    return Ok(expr);
                }

                items.push(expr);
            }

            while g.parse_token_fast(TokenKind::Comma).is_some() && !g.exhausted() {
                items.push(g.parse_compound_expr(0)?);
            }

            let children = g.nodes_with_joined_span(items, start);
            Ok(g.node_with_span(ast::Expr::Tuple(ast::TupleExpr { children }), token.span))
        })
    }

    /// Parse a list in brackets, e.g. `['a', 'b']`, whose items are separated
    /// by commas, with an optional comma after the last.
    fn parse_list_literal(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let children = self.in_tree(Delimiter::Bracket, None, |g| {
            let start = g.current_pos();
            let mut items = g.arena.vec();

            while !g.exhausted() {
                items.push(g.parse_compound_expr(0)?);

                if g.parse_token_fast(TokenKind::Comma).is_none() {
                    break;
                }
            }

            Ok(g.nodes_with_joined_span(items, start))
        })?;

        Ok(self.node_with_span(ast::Expr::Array(ast::ArrayExpr { children }), token.span))
    }

    /// Parse a mapping in braces, e.g. `{'a': 1, b: 2}`, whose pairs are
    /// separated by commas, with an optional comma after the last.
    fn parse_mapping(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let names = self.options.dialect.mapping_keys_can_be_names();
        let shorthand = self.options.dialect.mapping_values_can_be_left_out();

        let pairs = self.in_tree(Delimiter::Curly, None, |g| {
            let start = g.current_pos();
            let mut pairs = g.arena.vec();

            while let Some(first) = g.peek().copied() {
                // A key can be any name in [Nunjucks] and [Twig], even a
                // keyword, e.g. `{for: id}`. In Twig, the variable of that name
                // is its value when it has none, e.g. `{post}`.
                //
                // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/compiler.js#L318-L319
                // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
                let name = names
                    && matches!(first.kind, TokenKind::Ident | TokenKind::Keyword(_))
                    && match g.peek_second().map(|token| token.kind) {
                        Some(TokenKind::Colon) => true,
                        None | Some(TokenKind::Comma) => shorthand,
                        _ => false,
                    };

                let key = if name {
                    g.skip_token(); // `<name>` Skip the name.
                    let name = ast::Name::new(ast::Identifier::from(0u32));
                    g.node_with_span(ast::Expr::Var(ast::VarExpr { name }), first.span)
                } else {
                    g.parse_compound_expr(0)?
                };

                let value = if name && g.peek_kind() != Some(TokenKind::Colon) {
                    None
                } else {
                    g.parse_token(TokenKind::Colon)?;
                    Some(g.parse_compound_expr(0)?)
                };

                pairs.push(g.node_with_joined_span(ast::Pair { key, value }, first.span));

                if g.parse_token_fast(TokenKind::Comma).is_none() {
                    break;
                }
            }

            Ok(g.nodes_with_joined_span(pairs, start))
        })?;

        Ok(self.node_with_span(ast::Expr::Mapping(ast::MappingExpr { pairs }), token.span))
    }

    /// Parse the arguments of a call in parentheses, e.g. `(user, full=true)`,
    /// which are separated by commas, with an optional comma after the last.
    /// A named argument is written `name=value` in [Jinja], [Nunjucks] and
    /// [Twig], and also `name: value` in Twig.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L872-L926
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#keyword-arguments
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#named-arguments
    fn parse_call_args(&mut self) -> ParseResult<AstNodes<'a, ast::Arg<'a>>> {
        let colon = self.options.dialect.call_args_named_with_colon();
        let unpacking = self.options.dialect.call_args_can_be_unpacked();

        self.in_tree(Delimiter::Paren, None, |g| {
            let start = g.current_pos();
            let mut args = g.arena.vec();

            while let Some(token) = g.peek().copied() {
                let named = match g.peek_second().map(|token| token.kind) {
                    Some(TokenKind::Eq) => true,
                    Some(TokenKind::Colon) => colon,
                    _ => false,
                };

                let name = if named && token.kind.is_ident_like() {
                    let name = g.parse_name()?;
                    g.skip_token(); // `=` | `:` Skip the separator.
                    Some(name)
                } else {
                    None
                };

                // An argument is unpacked into several in [Jinja]: the items
                // of `*args`, and the pairs of `**kwargs`.
                //
                // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L899-L906
                let unpack = match token.kind {
                    _ if !unpacking => None,
                    TokenKind::Star => Some(ast::Unpack::Items),
                    TokenKind::StarStar => Some(ast::Unpack::Pairs),
                    _ => None,
                };
                if unpack.is_some() {
                    g.skip_token(); // `*` | `**` Skip the unpacking operator.
                }

                let value = g.parse_compound_expr(0)?;
                args.push(g.node_with_joined_span(
                    ast::Arg { name, value: Some(value), unpack },
                    token.span,
                ));

                if g.parse_token_fast(TokenKind::Comma).is_none() {
                    break;
                }
            }

            Ok(g.nodes_with_joined_span(args, start))
        })
    }

    /// Whether `token` starts a string that [Django] translates, i.e. `_`
    /// followed by a string in parentheses, e.g. `_("Name")`.
    ///
    /// [Django]: https://docs.djangoproject.com/en/stable/topics/i18n/translation/#string-literals-passed-to-tags-and-filters
    fn at_translated_string(&self, token: Token) -> bool {
        self.options.dialect.has_translated_strings()
            && self._source.hunk(token.span) == "_"
            && matches!(
                self.peek_raw(1).map(|token| token.kind),
                Some(TokenKind::Tree(Delimiter::Paren, 1))
            )
            && matches!(self.peek_raw(2).map(|token| token.kind), Some(TokenKind::Str))
    }

    /// Parse a value, i.e. an expression without any filters applied to it.
    fn parse_value(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;

        // Firstly, we have to get the initial part of the expression,
        // and then we can check if there are any additional parts in the
        // forms of either property accesses, indexing or method calls
        let (subject, subject_span) = self.track_span(|this| this.parse_expr_component(token))?;

        self.parse_singular_expr(subject, subject_span)
    }

    /// Provided an initial subject expression that is parsed by the parent
    /// caller, this function will check if there are any additional
    /// components to the expression; in the form of either property access,
    /// method calls, indexing, etc.
    fn parse_singular_expr(
        &mut self,
        mut subject: AstNode<'a, ast::Expr<'a>>,
        mut subject_span: ByteRange,
    ) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        // so here we need to peek to see if this is either a index_access, field access
        // or a function call...
        while let Some(token) = self.peek() {
            // @@Todo: do we need to explicitly break on whitespace??
            //
            // if there exists a space between the `subject` and the
            // next fragment of the expression, we treat them as explicitly
            // non-singular expressions.
            //
            //
            // if !token.span.is_right_before(subject_span) {
            //     break;
            // }

            subject = match token.kind {
                // Property access or method call
                TokenKind::Dot => self.parse_property_access(subject, subject_span)?,
                // Array index access syntax: ident[...]
                TokenKind::Tree(Delimiter::Bracket, _) => {
                    // A subscript is an expression in [Jinja], [Nunjucks] and
                    // [Twig], e.g. `items[i + 1]`, which can be a slice in
                    // Jinja and Twig, e.g. `items[1:]`.
                    //
                    // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L840-L870
                    // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L711-L721
                    // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/ExpressionParser/Infix/SquareBracketExpressionParser.php#L38-L45
                    let index = self.in_tree(Delimiter::Bracket, None, |g| {
                        if g.options.dialect.has_slices() {
                            g.parse_subscript()
                        } else if g.options.dialect.subscripts_take_operators() {
                            g.parse_compound_expr(0)
                        } else {
                            g.parse_expr()
                        }
                    })?;

                    self.node_with_joined_span(
                        ast::Expr::Index(ast::IndexExpr { subject, index }),
                        subject_span,
                    )
                }
                // A call in [Jinja], [Nunjucks] and [Twig], of a name, an
                // attribute or the result of a filter, e.g. `range(3)` or
                // `post.render(user)`.
                //
                // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#other-operators
                // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#function-calls
                // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#functions
                TokenKind::Tree(Delimiter::Paren, _) if self.options.dialect.has_calls() => {
                    let args = self.parse_call_args()?;
                    self.node_with_joined_span(
                        ast::Expr::Call(ast::CallExpr { subject, args }),
                        subject_span,
                    )
                }
                _ => break,
            };

            // We need to adjust the subject_span so we can compute whether
            // we're still "physically" connected to the end of the expression.
            subject_span = subject_span.join(self.previous_pos())
        }

        Ok(subject)
    }

    fn parse_unary_expr(&mut self, token: Token) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let op = self.node_with_span(
            match token.kind {
                TokenKind::Keyword(Keyword::Not) => ast::UnaryOp::Not,
                TokenKind::Minus => ast::UnaryOp::Neg,
                _ => unreachable!(),
            },
            token.span,
        );

        self.skip_fast(token.kind); // `<op>` Skip the operator token.

        // How tightly `not` binds follows [Django], [Jinja], [Liquid],
        // [Nunjucks] and [Twig].
        //
        // [Django]: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
        // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
        // [Liquid]: https://shopify.github.io/liquid/basics/operators/#order-of-operations
        // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L800-L808
        // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
        let expr = self.parse_compound_expr(self.options.dialect.prefix_binding_power(*op.body))?;
        Ok(self.node_with_joined_span(ast::Expr::Unary(ast::UnaryExpr { op, expr }), token.span))
    }

    fn parse_property_access(
        &mut self,
        subject: AstNode<'a, ast::Expr<'a>>,
        subject_span: ByteRange,
    ) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        self.skip_fast(TokenKind::Dot); // `<dot>` Skip the dot token.

        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;

        match token.kind {
            kind if kind.is_ident_like() => {
                let field = self.parse_name()?;

                Ok(self.node_with_joined_span(
                    ast::Expr::Access(ast::AccessExpr { subject, field }),
                    subject_span,
                ))
            }
            // A keyword is a name after a `.`, e.g. `params.for`.
            TokenKind::Keyword(keyword) => {
                self.skip_fast(TokenKind::Keyword(keyword)); // `<kw>` Skip the keyword token.

                let field = self.node_with_span(ast::Name::new(Identifier::from(0u32)), token.span);
                Ok(self.node_with_joined_span(
                    ast::Expr::Access(ast::AccessExpr { subject, field }),
                    subject_span,
                ))
            }
            TokenKind::Number(NumberFlags::Int) => {
                self.skip_fast(TokenKind::Number(NumberFlags::Int)); // `<number>` Skip the number token.

                let field = self.node_with_span(ast::Name::new(Identifier::from(0u32)), token.span);
                Ok(self.node_with_joined_span(
                    ast::Expr::Access(ast::AccessExpr { subject, field }),
                    subject_span,
                ))
            }
            _ => self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::Ident,
                Some(token.kind),
                token.span,
            ),
        }
    }

    fn parse_filter(
        &mut self,
        subject_span: ByteRange,
    ) -> ParseResult<AstNode<'a, ast::Filter<'a>>> {
        let name = self.parse_name()?;

        // A filter takes its arguments in parentheses in [Jinja], [Nunjucks]
        // and [Twig], e.g. `join(', ')`, as a call does.
        //
        // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#filters
        // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#filters
        // [Twig]: https://twig.symfony.com/doc/3.x/templates.html#filters
        let args = if self.options.dialect.filter_args_in_parentheses()
            && matches!(self.peek_kind(), Some(TokenKind::Tree(Delimiter::Paren, _)))
        {
            self.parse_call_args()?
        } else if self.parse_token_fast(TokenKind::Colon).is_some() {
            let start = self.current_pos();
            let mut args = self.arena.vec();
            args.push(self.parse_filter_arg()?);

            // [Liquid filters] take a list of arguments, e.g. `f: a, b`. In the
            // other dialects, a filter takes a single argument, so a `,` or a
            // `:` after it is Liquid's list.
            //
            // [Liquid filters]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
            if self.options.dialect.filter_supports_arg_list() {
                while self.parse_token_fast(TokenKind::Comma).is_some() {
                    args.push(self.parse_filter_arg()?);
                }
            } else if let Some(TokenKind::Comma | TokenKind::Colon) = self.peek_kind() {
                self.stop_at_foreign(Syntax::FilterArgLists);
            }

            self.nodes_with_joined_span(args, start)
        } else {
            AstNodes::empty(subject_span)
        };

        Ok(self.node_with_joined_span(ast::Filter { name, args }, subject_span))
    }

    /// Parse an argument of a filter, i.e. `a` in `x|f:a`. The argument has no
    /// filters of its own, so in `x|f:a|g` the filter `g` applies to `x|f:a`
    /// rather than to `a`. [Liquid filters] also take keyword arguments, e.g.
    /// `allow_false: true`.
    ///
    /// [Liquid filters]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
    fn parse_filter_arg(&mut self) -> ParseResult<AstNode<'a, ast::Arg<'a>>> {
        let start = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;

        let name = match self.peek_second() {
            Some(tok!(Colon))
                if self.options.dialect.filter_supports_arg_list()
                    && start.kind.is_ident_like() =>
            {
                let name = self.parse_name()?;
                self.skip_fast(TokenKind::Colon); // `:` Skip the colon token.
                Some(name)
            }
            _ => None,
        };

        let value = self.parse_value()?;
        Ok(self
            .node_with_joined_span(ast::Arg { name, value: Some(value), unpack: None }, start.span))
    }

    fn parse_bin_op(&mut self) -> (Option<ast::BinOp>, u8) {
        let token = self.peek();

        // check if there is a token that we can peek at ahead...
        if token.is_none() {
            return (None, 0);
        }

        let token = token.unwrap();
        match &token.kind {
            TokenKind::Keyword(Keyword::Contains) => (Some(ast::BinOp::Contains), 1),
            TokenKind::EqEq => (Some(ast::BinOp::Eq), 1),
            TokenKind::NotEq => (Some(ast::BinOp::NotEq), 1),
            TokenKind::EqEqEq => (Some(ast::BinOp::StrictEq), 1),
            TokenKind::NotEqEq => (Some(ast::BinOp::StrictNotEq), 1),
            TokenKind::Spaceship => (Some(ast::BinOp::Spaceship), 1),
            TokenKind::Lt => (Some(ast::BinOp::Lt), 1),
            TokenKind::LtEq => (Some(ast::BinOp::LtEq), 1),
            TokenKind::Gt => (Some(ast::BinOp::Gt), 1),
            TokenKind::GtEq => (Some(ast::BinOp::GtEq), 1),

            TokenKind::Plus => (Some(ast::BinOp::Add), 1),
            TokenKind::Minus => (Some(ast::BinOp::Sub), 1),
            TokenKind::Star => (Some(ast::BinOp::Mul), 1),
            TokenKind::Slash => (Some(ast::BinOp::Div), 1),
            TokenKind::SlashSlash => (Some(ast::BinOp::FloorDiv), 1),
            TokenKind::Percent => (Some(ast::BinOp::Mod), 1),
            TokenKind::StarStar => (Some(ast::BinOp::Pow), 1),
            TokenKind::Tilde => (Some(ast::BinOp::Concat), 1),
            TokenKind::QuestionQuestion => (Some(ast::BinOp::Coalesce), 1),
            TokenKind::DotDot => (Some(ast::BinOp::Range), 1),
            TokenKind::Ident => match (self._source.hunk(token.span), self.peek_second()) {
                ("matches", _) => (Some(ast::BinOp::Matches), 1),
                ("xor", _) => (Some(ast::BinOp::Xor), 1),
                ("b-and", _) => (Some(ast::BinOp::BitAnd), 1),
                ("b-or", _) => (Some(ast::BinOp::BitOr), 1),
                ("b-xor", _) => (Some(ast::BinOp::BitXor), 1),
                ("has", Some(second)) if second.kind == TokenKind::Ident => {
                    match self._source.hunk(second.span) {
                        "some" => (Some(ast::BinOp::HasSome), 2),
                        "every" => (Some(ast::BinOp::HasEvery), 2),
                        _ => (None, 0),
                    }
                }
                ("starts", Some(kw!(With))) => (Some(ast::BinOp::StartsWith), 2),
                ("ends", Some(kw!(With))) => (Some(ast::BinOp::EndsWith), 2),
                _ => (None, 0),
            },
            TokenKind::Question => match self.peek_second() {
                Some(tok!(Colon)) => (Some(ast::BinOp::Elvis), 2),
                _ => (None, 0),
            },

            TokenKind::Keyword(Keyword::And) => (Some(ast::BinOp::And), 1),
            TokenKind::Keyword(Keyword::Or) => (Some(ast::BinOp::Or), 1),
            TokenKind::Keyword(Keyword::In) => (Some(ast::BinOp::In), 1),
            TokenKind::Keyword(Keyword::Is) => match self.peek_second() {
                Some(kw!(Not)) => (Some(ast::BinOp::IsNot), 2),
                _ => (Some(ast::BinOp::Is), 1),
            },
            TokenKind::Keyword(Keyword::Not) => match self.peek_second() {
                Some(kw!(In)) => (Some(ast::BinOp::NotIn), 2),
                _ => (None, 0),
            },
            _ => (None, 0),
        }
    }

    fn parse_lit(&self) -> ParseResult<ast::Lit> {
        let token = self.current_token();

        match token.kind {
            TokenKind::Keyword(Keyword::False) => {
                self.skip_fast(TokenKind::Keyword(Keyword::False)); // `<false>` Skip the false token.
                Ok(ast::Lit::Bool(ast::BoolLit { value: false }))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.skip_fast(TokenKind::Keyword(Keyword::True)); // `<true>` Skip the true token.
                Ok(ast::Lit::Bool(ast::BoolLit { value: true }))
            }
            TokenKind::Str => {
                self.skip_fast(TokenKind::Str); // `<string>` Skip the string token.

                // Strings written one after another are one string in [Jinja],
                // e.g. `"a" "b"`.
                //
                // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L662-L669
                if self.options.dialect.adjacent_strings_concatenate() {
                    while self.peek_kind() == Some(TokenKind::Str) {
                        self.skip_fast(TokenKind::Str); // `<string>` Skip the next string.
                    }
                }

                Ok(ast::Lit::Str(ast::StrLit {}))
            }
            TokenKind::Number(flags) => {
                self.skip_fast(TokenKind::Number(flags)); // `<number>` Skip the number token.
                match flags {
                    NumberFlags::Int => Ok(ast::Lit::Int(ast::IntLit { value: 0 })),
                    NumberFlags::Float => Ok(ast::Lit::Float(ast::FloatLit { value: 0.0 })),
                }
            }
            _ => self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::Literal,
                Some(token.kind),
                token.span,
            ),
        }
    }

    /// Parse a `for` loop, or a Liquid `tablerow` loop, which is written the
    /// same way apart from its tags, and has no body for an empty loop.
    fn parse_for_loop(
        &mut self,
        kind: ast::LoopKind,
    ) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();
        let (opening, end) = match kind {
            ast::LoopKind::For => (Keyword::For, Keyword::EndFor),
            ast::LoopKind::TableRow => (Keyword::TableRow, Keyword::EndTableRow),
        };

        // The body for an empty loop comes after `{% empty %}` in [Django], and
        // after `{% else %}` in [Jinja], [Liquid], [Nunjucks] and [Twig].
        //
        // [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
        // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#for
        // [Liquid]: https://shopify.github.io/liquid/tags/iteration/#else
        // [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#for
        // [Twig]: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
        let dialect = self.options.dialect;
        let empty = match kind {
            ast::LoopKind::For => dialect.keyword(dialect.empty_loop_tag()),
            ast::LoopKind::TableRow => None,
        };

        // Parse the header first, which is within the current token.
        let ((target, iterator, reverse_modifier, params, guard), trim) =
            self.in_tag(Delimiter::Percent, |g| {
                g.parse_token(TokenKind::Keyword(opening))?;
                let target = g.parse_for_target()?;

                g.parse_token(TokenKind::Keyword(Keyword::In))?;

                // The iterator can have operators in [Jinja], [Nunjucks] and
                // [Twig], e.g. `{% for i in 1..n %}`, but isn't a conditional
                // expression, since an `if` after it is the loop's filter in
                // Jinja.
                //
                // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L240-L243
                // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L191
                // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/ForTokenParser.php#L39-L40
                let iterator = if g.options.dialect.loop_iterator_takes_operators() {
                    g.parse_compound_expr(ast::Dialect::CONDITIONAL_BINDING_POWER + 1)?
                } else {
                    g.parse_expr()?
                };
                let (reverse_modifier, params) = g.parse_loop_modifiers()?;

                // The loop's filter can have operators in [Jinja], e.g.
                // `{% for e in errors if e is string %}`.
                //
                // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L245-L246
                let guard = if g.parse_token_fast(TokenKind::Keyword(Keyword::If)).is_none() {
                    None
                } else if g.options.dialect.loop_filter_takes_operators() {
                    Some(g.parse_compound_expr(0)?)
                } else {
                    Some(g.parse_expr()?)
                };

                Ok((target, iterator, reverse_modifier, params, guard))
            })?;

        // Next, parse until we reach the end of the loop, or the body for an
        // empty loop, with the whitespace control of the tag there and the
        // text after its name.
        let footer = |g: &mut Self| {
            // The footer is found by its keyword, `<empty>` or `<end>`.
            let Some(TokenKind::Keyword(tag)) = g.peek_kind() else { unreachable!() };
            g.skip_token(); // `<empty>` | `<end>` Skip the empty or end token.
            Ok((g.trim(), g.parse_ignored_markup(tag, false)))
        };
        let (loop_body, ending_token, ending) = self.parse_body_until_block_footer(
            end,
            |kind| matches!(kind, TokenKind::Keyword(kwd) if kwd == end || Some(kwd) == empty),
            footer,
        )?;

        let (loop_empty, (empty_trim, empty_markup), (end_trim, end_markup)) =
            if ending_token != TokenKind::Keyword(end) {
                let (body, _, closing) = self.parse_body_until_block_footer(
                    end,
                    |kind| kind == TokenKind::Keyword(end),
                    footer,
                )?;

                (Some(body), ending, closing)
            } else {
                (None, (ast::TrimMarker::default(), None), ending)
            };

        let for_loop = self.node_with_joined_span(
            ast::For {
                kind,
                target,
                iterator,
                guard,
                loop_body,
                loop_empty,
                reverse_modifier,
                params,
                trim: ast::TrimTag { start: trim, end: end_trim },
                empty_trim,
                empty_markup,
                end_markup,
            },
            start,
        );
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::For(for_loop)), start))
    }

    /// Parse the modifiers after the iterator of a loop, in any order: the
    /// `reversed` modifier, and in [Liquid], parameters such as `limit: 2`.
    ///
    /// [Liquid]: https://shopify.github.io/liquid/tags/iteration/#for-parameters
    fn parse_loop_modifiers(
        &mut self,
    ) -> ParseResult<(Option<AstNode<'a, ast::Name>>, AstNodes<'a, ast::Arg<'a>>)> {
        let mut reverse_modifier = None;
        let mut params = self.arena.vec();
        let start = self.current_pos();

        loop {
            // The parameters can also be separated by commas.
            if self.options.dialect.has_loop_params() && !params.is_empty() {
                self.parse_token_fast(TokenKind::Comma);
            }

            match (self.peek().copied(), self.peek_second().copied()) {
                (Some(Token { kind: TokenKind::Keyword(Keyword::Reversed), span }), _)
                    if reverse_modifier.is_none() =>
                {
                    self.skip_fast(TokenKind::Keyword(Keyword::Reversed)); // `reversed` Skip the modifier.
                    reverse_modifier = Some(
                        self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), span),
                    );
                }
                (Some(Token { kind, span }), Some(tok!(Colon)))
                    if self.options.dialect.has_loop_params() && kind.is_ident_like() =>
                {
                    let name = self.parse_name()?;
                    self.skip_fast(TokenKind::Colon); // `:` Skip the colon token.
                    let value = self.parse_loop_param_value()?;

                    params.push(self.node_with_joined_span(
                        ast::Arg { name: Some(name), value: Some(value), unpack: None },
                        span,
                    ));
                }
                _ => break,
            }
        }

        Ok((reverse_modifier, self.nodes_with_joined_span(params, start)))
    }

    /// Parse the value of a Liquid loop parameter, e.g. `2` in `limit: 2`.
    fn parse_loop_param_value(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;

        // `offset: continue` starts where the last loop over the same items
        // stopped.
        if token.kind == TokenKind::Keyword(Keyword::Continue) {
            self.skip_fast(token.kind); // `continue` Skip the keyword.
            return Ok(self.node_with_span(
                ast::Expr::Var(ast::VarExpr { name: ast::Name::new(ast::Identifier::from(0u32)) }),
                token.span,
            ));
        }

        self.parse_value()
    }

    /// Parse the destructuring target of a for loop, i.e. the assigned variable
    /// or variables per iteration, e.g.
    /// ```ignore
    /// {% for x in y %}
    ///     ...
    /// {% endfor %}
    ///
    /// {% for x, y in z %}
    ///    ...
    /// {% endfor %}
    /// ```
    fn parse_for_target(&mut self) -> ParseResult<AstNode<'a, ast::ForTarget<'a>>> {
        let start = self.current_pos();
        let mut items = self.arena.vec();
        self.parse_target_names(&mut items)?;

        while self.parse_token_fast(TokenKind::Comma).is_some() {
            self.parse_target_names(&mut items)?;
        }

        let items = self.nodes_with_joined_span(items, start);
        Ok(self.node_with_joined_span(ast::ForTarget { items }, start))
    }

    /// Parse a name that a loop assigns to, or in [Jinja] the names that a
    /// tuple in parentheses unpacks into, e.g. `(a, b)` in
    /// `{% for (a, b), c in items %}`, which are added to `items` in order.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L473-L510
    fn parse_target_names(
        &mut self,
        items: &mut ArenaVec<'a, AstNode<'a, ast::Name>>,
    ) -> ParseResult<()> {
        if !(self.options.dialect.has_tuples()
            && matches!(self.peek_kind(), Some(TokenKind::Tree(Delimiter::Paren, _))))
        {
            items.push(self.parse_name()?);
            return Ok(());
        }

        self.in_tree(Delimiter::Paren, None, |g| {
            while !g.exhausted() {
                g.parse_target_names(items)?;

                if g.parse_token_fast(TokenKind::Comma).is_none() {
                    break;
                }
            }

            Ok(())
        })
    }

    /// Parse statements up to a `{% %}` tag whose keyword `stop` accepts, or
    /// the end of the input, without parsing that tag.
    fn parse_body_until(
        &mut self,
        stop: impl Fn(Keyword) -> bool,
    ) -> ParseResult<AstNode<'a, ast::Body<'a>>> {
        let start = self.current_pos();
        let mut contents = self.arena.vec();

        while let Some(token) = self.peek() {
            if token.kind.is_tag_tree()
                && let Some(TokenKind::Keyword(kwd)) = self.peek_raw(1).map(|t| t.kind)
                && stop(kwd)
            {
                break;
            }

            match self.parse_statement()? {
                Some(statement) => contents.push(statement),
                None => break,
            }
        }

        let contents = self.nodes_with_joined_span(contents, start);
        Ok(self.node_with_joined_span(ast::Body { contents }, start))
    }

    /// Parse an `if` block, or a Liquid `unless` block, which is written the
    /// same way apart from its opening and closing tags.
    fn parse_if_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let mut clauses = self.arena.vec();
        let mut otherwise = None;
        let (mut else_trim, mut else_markup) = (ast::TrimMarker::default(), None);
        let mut closing = None;
        let start = self.current_pos();

        let end = match self.peek_raw(1).map(|token| token.kind) {
            Some(TokenKind::Keyword(Keyword::Unless)) => Keyword::EndUnless,
            _ => Keyword::EndIf,
        };

        while let Some(token) = self.peek() {
            // Every clause body ends at the next tag of this block.
            if !matches!(token.kind, TokenKind::Tree(delimiter, len) if delimiter.is_tag() && len > 0)
            {
                break;
            }

            let token = self.peek_raw(1).copied().unwrap();

            // A clause can't come after `{% else %}`, and neither can a second
            // `{% else %}`, since the formatter would move or drop them.
            match token.kind {
                TokenKind::Keyword(kwd @ (Keyword::If | Keyword::Unless | Keyword::Elif))
                    if otherwise.is_none() =>
                {
                    let (condition, trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.skip_token();
                        g.parse_markup(|g| g.parse_compound_expr(0), |_, expr| expr)
                    })?;

                    let clause_kind = match kwd {
                        Keyword::If => bl_ast::ClauseKind::If,
                        Keyword::Unless => bl_ast::ClauseKind::Unless,
                        _ => bl_ast::ClauseKind::Elif,
                    };
                    let elseif = self._source.hunk(token.span) == "elseif";
                    let clause_body = self.parse_body_until(|kwd| {
                        kwd == end || matches!(kwd, Keyword::Elif | Keyword::Else)
                    })?;
                    clauses.push(self.node_with_joined_span(
                        ast::IfClause { kind: clause_kind, condition, clause_body, trim, elseif },
                        start,
                    ));
                }
                TokenKind::Keyword(Keyword::Else) if otherwise.is_none() => {
                    (else_markup, else_trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::Else))?;
                        Ok(g.parse_ignored_markup(Keyword::Else, true))
                    })?;

                    otherwise = Some(self.parse_body_until(|kwd| kwd == end)?);
                }
                TokenKind::Keyword(kwd) if kwd == end => {
                    let (markup, trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(end))?;
                        Ok(g.parse_ignored_markup(end, false))
                    })?;
                    closing = Some((trim, markup));
                    break;
                }
                _ => self.err_with_location(
                    ParseErrorKind::UnExpected,
                    ExpectedItem::empty(),
                    Some(token.kind),
                    token.span,
                )?,
            }
        }

        let Some((end_trim, end_markup)) = closing else {
            return self.err_with_location(
                ParseErrorKind::UnclosedTag(end),
                ExpectedItem::empty(),
                None,
                self.eof_pos(),
            );
        };

        let clauses = self.nodes_with_joined_span(clauses, start);
        let if_block = ast::If { clauses, otherwise, else_trim, end_trim, else_markup, end_markup };
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::If(if_block)), start))
    }

    /// Parse a Liquid `case` block, i.e.
    ///
    /// ```liquid
    /// {% case product.type %}
    ///     {% when "shirt", "hat" %} Apparel
    ///     {% else %} Other
    /// {% endcase %}
    /// ```
    fn parse_case_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();
        let is_clause = |kwd| matches!(kwd, Keyword::When | Keyword::Else | Keyword::EndCase);

        let (subject, trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Case))?;
            g.parse_markup(Self::parse_expr, |_, expr| expr)
        })?;

        let leading = self.parse_body_until(is_clause)?;

        let mut branches = self.arena.vec();
        let mut otherwise = None;
        let mut else_trim = ast::TrimMarker::default();
        let mut closing = None;
        let branches_start = self.current_pos();

        while let Some(token) = self.peek() {
            // Every clause body ends at the next tag of this block.
            if !matches!(token.kind, TokenKind::Tree(delimiter, len) if delimiter.is_tag() && len > 0)
            {
                break;
            }

            let token = self.peek_raw(1).copied().unwrap();

            // A clause can't come after `{% else %}`, and neither can a second
            // `{% else %}`, since the formatter would move or drop them.
            match token.kind {
                TokenKind::Keyword(Keyword::When) if otherwise.is_none() => {
                    let clause_start = self.current_pos();
                    let (values, trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.skip_token(); // `when` Skip the keyword.
                        g.parse_markup(Self::parse_when_values, |g, expr| {
                            let mut values = g.arena.vec();
                            values.push(expr);
                            g.nodes_with_span(values, expr.range())
                        })
                    })?;

                    let clause_body = self.parse_body_until(is_clause)?;
                    branches.push(self.node_with_joined_span(
                        ast::When { values, clause_body, trim },
                        clause_start,
                    ));
                }
                // Unlike an `if`'s, the `else` of a Liquid [`case`] takes no
                // text after it, see
                // [ast::Dialect::else_and_end_tags_ignore_markup].
                //
                // [`case`]: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/tags/case.rb#L146-L149
                TokenKind::Keyword(Keyword::Else) if otherwise.is_none() => {
                    ((), else_trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::Else))
                    })?;

                    otherwise = Some(self.parse_body_until(|kwd| kwd == Keyword::EndCase)?);
                }
                TokenKind::Keyword(Keyword::EndCase) => {
                    let (markup, trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::EndCase))?;
                        Ok(g.parse_ignored_markup(Keyword::EndCase, false))
                    })?;
                    closing = Some((trim, markup));
                    break;
                }
                _ => self.err_with_location(
                    ParseErrorKind::UnExpected,
                    ExpectedItem::empty(),
                    Some(token.kind),
                    token.span,
                )?,
            }
        }

        let Some((end_trim, end_markup)) = closing else {
            return self.err_with_location(
                ParseErrorKind::UnclosedTag(Keyword::EndCase),
                ExpectedItem::empty(),
                None,
                self.eof_pos(),
            );
        };

        let branches = self.nodes_with_joined_span(branches, branches_start);
        let case = self.node_with_joined_span(
            ast::Case {
                subject,
                leading,
                branches,
                otherwise,
                trim: ast::TrimTag { start: trim, end: end_trim },
                else_trim,
                end_markup,
            },
            start,
        );
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::Case(case)), start))
    }

    /// Parse the values of a `{% when %}` clause, which are separated by `,`
    /// or `or`.
    fn parse_when_values(&mut self) -> ParseResult<AstNodes<'a, ast::Expr<'a>>> {
        let start = self.current_pos();
        let mut values = self.arena.vec();
        values.push(self.parse_expr()?);

        while self
            .parse_token_fast(TokenKind::Comma)
            .or_else(|| self.parse_token_fast(TokenKind::Keyword(Keyword::Or)))
            .is_some()
        {
            values.push(self.parse_expr()?);
        }

        Ok(self.nodes_with_joined_span(values, start))
    }

    /// Parse a Twig [`guard`] block, which renders its body if the function,
    /// filter or test that it names exists, and else the body after its
    /// `{% else %}`, i.e.
    ///
    /// ```twig
    /// {% guard function importmap %}
    ///     {{ importmap('app') }}
    /// {% else %}
    ///     <script src="app.js"></script>
    /// {% endguard %}
    /// ```
    ///
    /// [`guard`]: https://twig.symfony.com/doc/3.x/tags/guard.html
    fn parse_guard_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();

        let ((kind, name), trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Guard))?;
            let kind = g.parse_guard_kind()?;
            Ok((kind, g.parse_guard_name(kind)?))
        })?;

        // The body ends at `{% else %}` or `{% endguard %}`, and the body after
        // `{% else %}` at `{% endguard %}`.
        let is_else = |kind: TokenKind| kind == TokenKind::Keyword(Keyword::Else);
        let is_end = |kind: TokenKind| kind == TokenKind::Keyword(Keyword::EndGuard);
        let footer = |g: &mut Self| {
            g.skip_token(); // `else` | `endguard` Skip the else or end token.
            Ok(g.trim())
        };

        let (guard_body, ending_token, ending_trim) = self.parse_body_until_block_footer(
            Keyword::EndGuard,
            |kind| is_else(kind) || is_end(kind),
            footer,
        )?;

        let (otherwise, else_trim, end_trim) = if is_else(ending_token) {
            let (body, _, end_trim) =
                self.parse_body_until_block_footer(Keyword::EndGuard, is_end, footer)?;
            (Some(body), ending_trim, end_trim)
        } else {
            (None, ast::TrimMarker::default(), ending_trim)
        };

        let guard = self.node_with_joined_span(
            ast::Guard {
                kind,
                name,
                guard_body,
                otherwise,
                trim: ast::TrimTag { start: trim, end: end_trim },
                else_trim,
            },
            start,
        );
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::Guard(guard)), start))
    }

    /// Parse the kind of callable that a `guard` checks for, which is
    /// `function`, `filter` or `test` in [Twig].
    ///
    /// [Twig]: https://twig.symfony.com/doc/3.x/tags/guard.html
    fn parse_guard_kind(&mut self) -> ParseResult<ast::GuardKind> {
        let token = self.peek().copied();
        let kind = match token.map(|token| (token.kind, self._source.hunk(token.span))) {
            Some((TokenKind::Ident, "function")) => ast::GuardKind::Function,
            Some((TokenKind::Ident, "filter")) => ast::GuardKind::Filter,
            Some((TokenKind::Ident, "test")) => ast::GuardKind::Test,
            _ => {
                return self.err_with_location(
                    ParseErrorKind::GuardKind,
                    ExpectedItem::empty(),
                    token.map(|token| token.kind),
                    token.map_or_else(|| self.eof_pos(), |token| token.span),
                );
            }
        };

        self.skip_fast(TokenKind::Ident); // `function` | `filter` | `test` Skip the kind.
        Ok(kind)
    }

    /// Parse the name of the callable that a `guard` checks for, which can be
    /// any name, e.g. `raw`. [Twig] reads a second name after a test's as
    /// part of it, e.g. `same as`, so the name spans both.
    ///
    /// [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/GuardTokenParser.php#L34-L40
    fn parse_guard_name(&mut self, kind: ast::GuardKind) -> ParseResult<AstNode<'a, ast::Name>> {
        let start = self.current_pos();
        let at_name = |g: &Self| match g.peek_kind() {
            Some(TokenKind::Ident) => true,
            Some(TokenKind::Keyword(keyword)) => {
                keyword.identifier_like() || g.keyword_is_name(keyword)
            }
            _ => false,
        };

        if !at_name(self) {
            return self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::Ident,
                self.peek_kind(),
                self.expected_pos(),
            );
        }
        self.skip_token(); // `<name>` Skip the name.

        if kind == ast::GuardKind::Test && at_name(self) {
            self.skip_token(); // `<name>` Skip the second word of the test's name.
        }

        Ok(self.node_with_joined_span(ast::Name::new(ast::Identifier::from(0u32)), start))
    }

    fn parse_break_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Break))?;
            Ok(g.node_with_span(ast::Statement::Tag(ast::Tag::Break(ast::Break {})), g.range()))
        })
    }

    fn parse_continue_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Continue))?;
            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Continue(ast::Continue {})),
                g.range(),
            ))
        })
    }

    fn parse_with_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let token = *self.current_token();

        // Parse the header first, we should get `with <assignments>`.
        let ((assignments, value, only), trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::With))?;

            // A [Twig] `with` takes an expression whose value is a mapping of
            // its variables, which `only` makes the only ones there are, e.g.
            // `{% with {a: 1} only %}`.
            //
            // [Twig]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/WithTokenParser.php#L33-L36
            if g.options.dialect.with_takes_an_expression() {
                let value = if g.exhausted() { None } else { Some(g.parse_compound_expr(0)?) };
                let only = g.peek().is_some_and(|token| {
                    token.kind == TokenKind::Ident && g._source.hunk(token.span) == "only"
                });
                if only {
                    g.skip_fast(TokenKind::Ident); // `only` Skip the modifier.
                }

                return Ok((g.nodes_with_span(g.arena.vec(), g.range()), value, only));
            }

            Ok((g.parse_assignments()?, None, false))
        })?;

        // Now parse a bunch of general statements until we reach the end of the block.
        let (block_body, _, end_trim) = self.parse_body_until_block_footer(
            Keyword::EndWith,
            |kind| matches!(kind, TokenKind::Keyword(Keyword::EndWith)),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndWith)); // `<endwith>` Skip the end token.
                let _ = g.parse_token_fast(TokenKind::Ident);

                Ok(g.trim())
            },
        )?;

        let with = self.node_with_joined_span(
            ast::With {
                assignments,
                value,
                only,
                block_body,
                kind: ast::AssignmentKind::With,
                trim: ast::TrimTag { start: trim, end: end_trim },
            },
            token.span,
        );
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::With(with)), token.span))
    }

    fn parse_block_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let token = *self.current_token();

        // Parse the header first, we should get `block <name>`.
        let ((label, scoped, required, value), trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Block))?;

            // If the next token is a string literal, we still accept it but
            // we emit a warning since it technically not a legal block name.
            if g.peek_kind() == Some(TokenKind::Str) {
                let span = g.current_pos();
                g.add_warning(ParseWarning::new(
                    ParseWarningKind::BlockLabelIsStringLiteral,
                    g.make_span(span),
                ));
            }

            let label = g.parse_block_name()?;

            // A [Jinja] block can be `scoped` and `required`, in that order.
            //
            // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L296-L300
            let modifiers = g.options.dialect.has_block_modifiers();
            let modifier = |g: &mut Self, name: &str| {
                let found = modifiers
                    && g.peek().is_some_and(|token| {
                        token.kind == TokenKind::Ident && g._source.hunk(token.span) == name
                    });
                if found {
                    g.skip_fast(TokenKind::Ident); // `scoped` | `required` Skip the modifier.
                }

                found
            };
            let scoped = modifier(g, "scoped");
            let required = modifier(g, "required");

            // A [Twig] block can be a single tag whose content is the value of
            // an expression, e.g. `{% block title page_title|title %}`.
            //
            // [Twig]: https://twig.symfony.com/doc/3.x/tags/extends.html#block-shortcuts
            let value = if g.options.dialect.has_block_shortcuts() && !g.exhausted() {
                Some(g.parse_compound_expr(0)?)
            } else {
                None
            };

            Ok((label, scoped, required, value))
        })?;

        if let Some(value) = value {
            let shortcut = ast::Tag::BlockShortcut(ast::BlockShortcut { label, value });
            return Ok(self.node_with_joined_span(ast::Statement::Tag(shortcut), token.span));
        }

        // Now parse a bunch of general statements until we reach the end of the block.
        let (block_body, _, (end_label, end_trim)) = self.parse_body_until_block_footer(
            Keyword::EndBlock,
            |kind| matches!(kind, TokenKind::Keyword(Keyword::EndBlock)),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndBlock)); // `<endblock>` Skip the endblock token.

                // @@Todo: check if the name matches the label, if the labels mismatch then
                // we should generate an error.
                let end_label = if g.peek().is_some() { Some(g.parse_block_name()?) } else { None };
                Ok((end_label, g.trim()))
            },
        )?;

        let block = self.node_with_joined_span(
            ast::Block {
                label: Some(label),
                scoped,
                required,
                block_body,
                end_label,
                trim: ast::TrimTag { start: trim, end: end_trim },
            },
            token.span,
        );
        Ok(self.node_with_joined_span(ast::Statement::Tag(ast::Tag::Block(block)), token.span))
    }

    /// Parse the name of a block, which is any word up to a space in [Django],
    /// e.g. `table.caption`, and a name or a string in the other dialects.
    ///
    /// [Django]: https://github.com/django/django/blob/6.1.2/django/template/loader_tags.py#L221-L224
    fn parse_block_name(&mut self) -> ParseResult<AstNode<'a, ast::Name>> {
        let Some(first) = self.peek().copied() else {
            return self.parse_name_or_string();
        };
        if !self.options.dialect.block_names_are_words() || first.kind == TokenKind::Str {
            return self.parse_name_or_string();
        }

        // The word's tokens are next to each other, e.g. `table`, `.` and
        // `caption`, as a space would end it.
        self.skip_token(); // `<token>` Skip the first token of the word.
        let mut span = first.span;
        while let Some(next) = self.peek().copied()
            && next.span.start() == span.end() + 1
        {
            self.skip_token(); // `<token>` Skip the next token of the word.
            span = span.join(next.span);
        }

        Ok(self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), span))
    }

    fn parse_raw_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();

        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Raw))
        })?;

        // The lexer reads the contents of the block as text, so this only
        // finds the end of the block.
        let (block_body, _, _) = self.parse_body_until_block_footer(
            Keyword::EndRaw,
            |kind| matches!(kind, TokenKind::Keyword(Keyword::EndRaw)),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndRaw)); // `<endraw>` Skip the endraw token.

                // The block is kept as it is written, so the rest of the tag
                // needn't be held.
                g.parse_ignored_markup(Keyword::EndRaw, false);
                Ok(())
            },
        )?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Raw(ast::Raw { block_body })),
            start,
        ))
    }

    /// Parse a block that isn't rendered, up to its `end` tag, e.g.
    /// `{% endcomment %}`, which it must have, as any other block.
    fn parse_comment_block(
        &mut self,
        end: Keyword,
    ) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();

        while let Some(token) = self.peek() {
            // If it's a percent tree (and of length 1), then we can check if it's the end
            // of the block.
            if token.kind.is_tag_tree() {
                let maybe_token = self.peek_raw(1).map(|tok| tok.kind);
                if maybe_token == Some(TokenKind::Keyword(end)) {
                    // The comment is kept as it is written, so the rest of the
                    // tag needn't be held.
                    self.in_tree(Delimiter::Percent, None, |g| {
                        g.parse_token(TokenKind::Keyword(end))?;
                        g.parse_ignored_markup(end, false);
                        Ok(())
                    })?;

                    let comment = ast::Statement::Comment(ast::Comment {});
                    return Ok(self.node_with_joined_span(comment, start));
                }
            }

            self.skip_token();
        }

        self.err_with_location(
            ParseErrorKind::UnclosedTag(end),
            ExpectedItem::empty(),
            None,
            self.eof_pos(),
        )
    }

    /// Parse statements until the footer tag that `peek_fn` accepts, and then
    /// parse the footer with `g`. Returns the body, the kind of the footer's
    /// first token, and what `g` returns. If the footer is missing, the error
    /// names `closing`, the keyword of the tag that closes the block.
    fn parse_body_until_block_footer<U>(
        &mut self,
        closing: Keyword,
        peek_fn: impl Fn(TokenKind) -> bool,
        g: impl FnMut(&mut Self) -> ParseResult<U>,
    ) -> ParseResult<(AstNode<'a, ast::Body<'a>>, TokenKind, U)> {
        let mut statements = self.arena.vec();
        let start = self.current_pos();
        let mut end: Option<_> = None;

        while let Some(token) = self.peek() {
            // If it's a percent tree (and of length 1), then we can check if it's the end
            // of the block.
            if token.kind.is_tag_tree() {
                let maybe_token = self.peek_raw(1);
                if let Some(inner) = maybe_token
                    && peek_fn(inner.kind)
                {
                    // Parse the end of the block now and record the span.
                    end = Some((*inner, self.in_tree(Delimiter::Percent, None, g)?));
                    break;
                }
            }

            if let Some(statement) = self.parse_statement()? {
                statements.push(statement);
            } else {
                break;
            }
        }

        // If there is no end, then we generate an error about an un-closed
        // block.
        if let Some((end, footer)) = end {
            let span = start.join(end.span);
            let contents = self.nodes_with_span(statements, span);
            Ok((self.node_with_joined_span(ast::Body { contents }, span), end.kind, footer))
        } else {
            self.err_with_location(
                ParseErrorKind::UnclosedTag(closing),
                ExpectedItem::empty(),
                None,
                self.eof_pos(),
            )
        }
    }

    fn parse_args(&mut self) -> ParseResult<AstNodes<'a, ast::Arg<'a>>> {
        let mut args = self.arena.vec();
        let start = self.current_pos();

        while self.peek().is_some() {
            // Liquid separates a tag's arguments with commas, e.g. in [`cycle`]
            // and [`render`].
            //
            // [`cycle`]: https://shopify.github.io/liquid/tags/iteration/#cycle
            // [`render`]: https://shopify.github.io/liquid/tags/template/#render-parameters
            if self.options.dialect.requires_comma_separated_args() {
                self.parse_token_fast(TokenKind::Comma);
            }

            match self.parse_arg() {
                Ok(Some(arg)) => args.push(arg),
                Ok(None) => break,
                Err(err) => return Err(err),
            }
        }

        self.stop_at_foreign_args();
        Ok(self.nodes_with_joined_span(args, start))
    }

    /// Parse a Liquid `render` tag, i.e.
    ///
    /// ```liquid
    /// {% render "card", product: product %}
    /// {% render "card" with featured as product %}
    /// {% render "card" for products as product %}
    /// ```
    fn parse_render_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Render))?;
            let template = g.parse_expr()?;

            let (with_value, for_value) = match g.peek_kind() {
                Some(TokenKind::Keyword(Keyword::With)) => {
                    g.skip_fast(TokenKind::Keyword(Keyword::With)); // `with` Skip the keyword.
                    (Some(g.parse_expr()?), None)
                }
                Some(TokenKind::Keyword(Keyword::For)) => {
                    g.skip_fast(TokenKind::Keyword(Keyword::For)); // `for` Skip the keyword.
                    (None, Some(g.parse_expr()?))
                }
                _ => (None, None),
            };

            let alias = if (with_value.is_some() || for_value.is_some())
                && g.parse_token_fast(TokenKind::Keyword(Keyword::As)).is_some()
            {
                Some(g.parse_name()?)
            } else {
                None
            };

            let args = g.parse_args()?;

            let render = g.node_with_span(
                ast::Render { template, with_value, for_value, alias, args },
                g.range(),
            );
            Ok(g.node_with_span(ast::Statement::Tag(ast::Tag::Render(render)), g.range()))
        })
    }

    /// Parse a Liquid `{% liquid %}` tag. The lexer gives each of its lines a
    /// `{% %}` tree, so the lines are parsed like any other tags, up to the
    /// tree for its `%}`.
    fn parse_liquid_tag(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let start = self.current_pos();

        // The lines of the tag are tags of their own, up to the end of the tag.
        let block_body = self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Liquid))?;
            g.parse_body_until(|_| false)
        })?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Liquid(ast::LiquidTag { block_body })),
            start,
        ))
    }

    /// Parse a Liquid `capture` block, i.e.
    /// `{% capture greeting %}Hello {{ name }}{% endcapture %}`.
    fn parse_capture_block(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        let token = *self.current_token();

        let (name, trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Capture))?;

            g.parse_name_or_string()
        })?;

        let (block_body, _, (end_trim, end_markup)) = self.parse_body_until_block_footer(
            Keyword::EndCapture,
            |kind| kind == TokenKind::Keyword(Keyword::EndCapture),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndCapture)); // `<endcapture>` Skip the end token.
                Ok((g.trim(), g.parse_ignored_markup(Keyword::EndCapture, false)))
            },
        )?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Capture(ast::Capture {
                name,
                block_body,
                trim: ast::TrimTag { start: trim, end: end_trim },
                end_markup,
            })),
            token.span,
        ))
    }

    /// Parse an [`extends`] statement, i.e.
    ///
    ///
    /// ```html
    /// {% extends "base.html" %}
    /// ```
    ///
    /// [`extends`]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#extends
    fn parse_extends_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Extends))?;
            let template = g.parse_template_name()?;

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Extends(ast::Extends { template, trim: g.trim() })),
                g.range(),
            ))
        })
    }

    /// Parse an [`include`] statement, or Jekyll's [`include_relative`], i.e.
    ///
    /// ```html
    /// {% include "header.html" with title="Header" %}
    /// {% include_relative sections/intro.md %}
    /// ```
    ///
    /// [`include`]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#include
    /// [`include_relative`]: https://jekyllrb.com/docs/includes/#including-files-relative-to-another-file
    fn parse_include_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        // Parse the header first, we should get `block <name>`.
        self.in_tree(Delimiter::Percent, None, |g| {
            let relative =
                g.parse_token_fast(TokenKind::Keyword(Keyword::IncludeRelative)).is_some();
            if !relative {
                g.parse_token(TokenKind::Keyword(Keyword::Include))?;
            }

            // Jekyll's [`include`] can name the file with a path, which the
            // lexer reads as one token, e.g. `nav/{{ name }}.html`.
            //
            // [`include`]: https://jekyllrb.com/docs/includes/#using-variables-names-for-the-include-file
            let template = match g.peek().copied() {
                Some(Token { kind: TokenKind::Path, span }) => {
                    g.skip_fast(TokenKind::Path); // `<path>` Skip the path token.
                    g.node_with_span(ast::Expr::Path(ast::PathExpr {}), span)
                }
                _ => g.parse_template_name()?,
            };

            // A [Jinja] `include` can end with `with context` or
            // `without context`, rather than the variables to pass.
            //
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
            let with_context = if g.options.dialect.include_takes_context() {
                g.parse_import_context()
            } else {
                None
            };

            // With comma separated arguments, the comma after the template
            // starts them rather than `with`, as in Liquid's [`render`], e.g.
            // `{% include "card", product: product %}`. Jekyll's parameters
            // are arguments too, written `name=value`, e.g. `title="Docs"`.
            //
            // [`render`]: https://shopify.github.io/liquid/tags/template/#render-parameters
            let context = if with_context.is_some() {
                g.nodes_with_span(g.arena.vec(), g.range())
            } else if g.parse_token_fast(TokenKind::Keyword(Keyword::With)).is_some()
                || g.options.dialect.requires_comma_separated_args()
            {
                g.parse_args()?
            } else {
                g.stop_at_foreign_args();
                g.nodes_with_span(g.arena.vec(), g.range())
            };

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Include(ast::Include {
                    template,
                    context,
                    with_context,
                    relative,
                })),
                g.range(),
            ))
        })
    }

    fn parse_load_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            let name = g.parse_name()?;
            // g.parse_token(TokenKind::Keyword(Keyword::Load))?;

            // @@Cleanup: technically not fully correct since we should only support
            // identifiers, but we can validate this on `ast_expand`.
            let args = g.parse_args()?;

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Generic(ast::GenericTag { name, args })),
                g.range(),
            ))
        })
    }

    /// Parse the template that a tag such as `include` names, which is an
    /// expression in [Jinja], [Nunjucks] and Twig's [`include`], [`extends`]
    /// and [`import`], e.g. `"icons/" ~ icon ~ ".svg"`, or Twig's `_self`.
    /// It is a value with its filters in Django and Liquid.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L328-L366
    /// [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L285-L437
    /// [`include`]: https://twig.symfony.com/doc/3.x/tags/include.html
    /// [`extends`]: https://twig.symfony.com/doc/3.x/tags/extends.html#conditional-inheritance
    /// [`import`]: https://github.com/twigphp/Twig/blob/v3.30.0/src/TokenParser/ImportTokenParser.php#L31
    fn parse_template_name(&mut self) -> ParseResult<AstNode<'a, ast::Expr<'a>>> {
        if self.options.dialect.template_names_take_operators() {
            self.parse_compound_expr(0)
        } else {
            self.parse_expr()
        }
    }

    /// Parse a Jinja [`import`] statement, i.e.
    ///
    /// ```html
    /// {% import "forms.html" as forms %}
    /// ```
    ///
    /// [`import`]: https://jinja.palletsprojects.com/en/stable/templates/#import
    fn parse_import_statement(&mut self) -> ParseResult<AstNode<'a, ast::Statement<'a>>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Import))?;
            let template = g.parse_template_name()?;

            let names = if g.parse_token_fast(TokenKind::Keyword(Keyword::As)).is_some() {
                g.parse_names()?
            } else {
                // This is effectively a dummy range, since we don't have an alias.
                g.nodes_with_span(g.arena.vec(), g.range())
            };

            // An `import` can end with `with context` or `without context` in
            // [Jinja] and [Nunjucks].
            //
            // [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#import-context-behavior
            // [Nunjucks]: https://github.com/mozilla/nunjucks/blob/v3.2.4/nunjucks/src/parser.js#L263-L302
            let with_context = if g.options.dialect.import_takes_context() {
                g.parse_import_context()
            } else {
                None
            };

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Import(ast::Import {
                    template,
                    names,
                    with_context,
                })),
                g.range(),
            ))
        })
    }

    /// Parse the `with context` or `without context` that an `import` or an
    /// `include` can end with, which says whether the template that it names
    /// sees this one's variables, see [ast::Dialect::import_takes_context].
    fn parse_import_context(&mut self) -> Option<bool> {
        let with_context = self.at_import_context()?;
        self.skip(2); // `with context` or `without context`.
        Some(with_context)
    }

    /// Whether `with context` or `without context` is next, as `Some` of
    /// whether it is `with`, see [Parser::parse_import_context].
    fn at_import_context(&self) -> Option<bool> {
        let with_context = match self.peek()? {
            Token { kind: TokenKind::Keyword(Keyword::With), .. } => true,
            Token { kind: TokenKind::Ident, span } if self._source.hunk(*span) == "without" => {
                false
            }
            _ => return None,
        };

        match self.peek_second()? {
            Token { kind: TokenKind::Ident, span } if self._source.hunk(*span) == "context" => {
                Some(with_context)
            }
            _ => None,
        }
    }

    /// Parse the names after `as` in an `import`, up to the end of the tag or
    /// its `with context` or `without context`.
    fn parse_names(&mut self) -> ParseResult<AstNodes<'a, ast::Name>> {
        let mut names = self.arena.vec();
        let start = self.current_pos();

        while self.peek().is_some() && self.at_import_context().is_none() {
            match self.parse_name() {
                Ok(name) => names.push(name),
                Err(err) => {
                    self.add_error(err);
                    break;
                }
            }
        }

        Ok(self.nodes_with_joined_span(names, start))
    }

    /// Parse a name, which can also be written as a string, e.g. the `"x"` in
    /// `{% capture "x" %}`.
    fn parse_name_or_string(&mut self) -> ParseResult<AstNode<'a, ast::Name>> {
        match self.peek().copied() {
            Some(Token { kind: TokenKind::Str, span }) => {
                self.skip_fast(TokenKind::Str); // `<string>` Skip the string token.
                Ok(self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), span))
            }
            _ => self.parse_name(),
        }
    }

    fn parse_name(&mut self) -> ParseResult<AstNode<'a, ast::Name>> {
        match self.peek() {
            Some(Token { kind: TokenKind::Ident, span }) => {
                self.skip_fast(TokenKind::Ident); // `<ident>` Skip the identifier token.

                // @@Todo: actually interpolate the identifiers.
                Ok(self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), *span))
            }
            Some(Token { kind: kind @ TokenKind::Keyword(kw), span }) if kw.identifier_like() => {
                self.skip_fast(*kind); // `<kw>` Skip the keyword token.

                Ok(self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), *span))
            }
            token => self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::Ident,
                token.map(|tok| tok.kind),
                self.expected_pos(),
            ),
        }
    }

    fn parse_arg(&mut self) -> ParseResult<Option<AstNode<'a, ast::Arg<'a>>>> {
        match (self.peek().copied(), self.peek_second().copied()) {
            (Some(Token { kind, span }), Some(tok!(Eq))) if kind.is_ident_like() => {
                let name = self.parse_name()?;
                self.parse_token(TokenKind::Eq)?;

                let value = self.parse_expr()?;

                Ok(Some(self.node_with_joined_span(
                    ast::Arg { name: Some(name), value: Some(value), unpack: None },
                    span,
                )))
            }
            // Liquid writes a named argument `name: value`, as in [`render`].
            // The name can also be a string, e.g. the group of a [`cycle`] in
            // `{% cycle "group": "a", "b" %}`.
            //
            // [`render`]: https://shopify.github.io/liquid/tags/template/#render-parameters
            // [`cycle`]: https://shopify.github.io/liquid/tags/iteration/#cycle
            (Some(Token { kind, span }), Some(tok!(Colon)))
                if self.options.dialect.named_args_use_colon()
                    && (kind.is_ident_like() || kind == TokenKind::Str) =>
            {
                let name = self.parse_name_or_string()?;

                self.skip_fast(TokenKind::Colon); // `:` Skip the colon token.
                let value = self.parse_expr()?;

                Ok(Some(self.node_with_joined_span(
                    ast::Arg { name: Some(name), value: Some(value), unpack: None },
                    span,
                )))
            }
            (Some(Token { kind, span }), _) if self.starts_expr(kind) => {
                let value = self.parse_expr()?;
                Ok(Some(self.node_with_joined_span(
                    ast::Arg { name: None, value: Some(value), unpack: None },
                    span,
                )))
            }
            _ => Ok(None),
        }
    }

    /// Whether `kind` starts an expression. A group or a tuple in parentheses,
    /// a list in brackets and a mapping in braces, in [Jinja], [Nunjucks] and
    /// [Twig], do too.
    ///
    /// [Jinja]: https://jinja.palletsprojects.com/en/stable/templates/#literals
    /// [Nunjucks]: https://mozilla.github.io/nunjucks/templating.html#expressions
    /// [Twig]: https://twig.symfony.com/doc/3.x/templates.html#literals
    fn starts_expr(&self, kind: TokenKind) -> bool {
        match kind {
            TokenKind::Tree(Delimiter::Paren, _) => self.options.dialect.has_grouping(),
            TokenKind::Tree(Delimiter::Bracket, _) => self.options.dialect.has_lists(),
            TokenKind::Tree(Delimiter::Curly, _) => self.options.dialect.has_mappings(),
            kind => kind.starts_expr(),
        }
    }

    /// Parse the assignments of a `{% with %}` block, which are separated by
    /// commas in [Jinja], and by spaces in Django.
    ///
    /// [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L273-L284
    fn parse_assignments(&mut self) -> ParseResult<AstNodes<'a, ast::Assignment<'a>>> {
        let commas = self.options.dialect.with_assignments_take_operators();
        let mut assignments = self.arena.vec();
        let start = self.current_pos();

        while self.peek().is_some() {
            match self.parse_assignment() {
                Ok(Some(assignment)) => assignments.push(assignment),
                Ok(None) => break,
                Err(err) => {
                    self.add_error(err);
                    break;
                }
            }

            if commas && self.parse_token_fast(TokenKind::Comma).is_none() {
                break;
            }
        }

        Ok(self.nodes_with_joined_span(assignments, start))
    }

    fn parse_assignment(&mut self) -> ParseResult<Option<AstNode<'a, ast::Assignment<'a>>>> {
        let position = self.cursor.position();
        let start = self.current_pos();

        match (self.peek(), self.peek_second()) {
            (Some(Token { kind, .. }), Some(tok!(Eq))) if kind.is_ident_like() => {
                let name = self.parse_name()?;
                self.skip_fast(TokenKind::Eq); // `<eq>` Skip the assignment operator token.

                // A value can have operators in [Jinja], e.g. `classes or []`.
                //
                // [Jinja]: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L273-L284
                let value = if self.options.dialect.with_assignments_take_operators() {
                    self.parse_compound_expr(0)?
                } else {
                    self.parse_expr()?
                };
                Ok(Some(self.node_with_joined_span(ast::Assignment { name, value }, start)))
            }
            // [Django]'s `value as name`, e.g.
            // `{% with business.employees.count as total %}`.
            //
            // [Django]: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#with
            _ if self.options.dialect.has_as_assignments() => {
                let Some(value) = self.peek_resultant_fn(|g| g.parse_expr()) else {
                    return Ok(None);
                };

                let maybe_token = self.peek_kind();

                if let Some(TokenKind::Keyword(Keyword::As)) = maybe_token {
                    self.skip_fast(TokenKind::Keyword(Keyword::As)); // `as` Skip the operator token.
                    let name = self.parse_name()?;

                    Ok(Some(self.node_with_joined_span(ast::Assignment { name, value }, start)))
                } else {
                    // Reset the cursor to the start position.
                    unsafe {
                        self.cursor.set_pos(position);
                    }
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Exhaust the current [TokenCursor] until the end of the input.
    ///
    /// This is an auxiliary operation for the parser for wh
    fn exhaust(&mut self) {
        let end = self.cursor.len();

        unsafe {
            self.cursor.set_pos(end);
        }
    }

    fn exhausted(&self) -> bool {
        self.cursor.position() == self.cursor.len()
    }
}

/// The whitespace control markers of `tag`, which are any of `markers` right
/// inside its two character delimiters.
fn tag_trim(tag: &str, markers: &[char]) -> ast::TrimMarker {
    let inner = tag.get(2..tag.len().saturating_sub(2)).unwrap_or_default();
    let marker = |c: Option<char>| c.filter(|c| markers.contains(c));

    ast::TrimMarker { left: marker(inner.chars().next()), right: marker(inner.chars().next_back()) }
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_tag_trim() {
        let both = ast::TrimMarker { left: Some('-'), right: Some('-') };
        assert_eq!(tag_trim("{%- if x -%}", &['-']), both);
        assert_eq!(tag_trim("{{- x }}", &['-']).left, Some('-'));
        assert_eq!(tag_trim("{% if x %}", &['-']), ast::TrimMarker::default());

        // Django has no markers.
        assert_eq!(tag_trim("{%- if x -%}", &[]), ast::TrimMarker::default());
    }
}
