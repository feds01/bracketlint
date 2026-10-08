//! The parser module is responsible for converting a stream of tokens into an
//! abstract syntax tree.
//!
//! Based on several documentation sources:
//!
//! - https://jinja.palletsprojects.com/en/stable/templates/

use std::cell::Cell;

use bl_ast::{
    self as ast, AstNode, AstNodes, ByteRange, Identifier, Keyword, LocalSpanMap, SourceId, Span,
    SpannedSource, VarExpr,
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
use thin_vec::{ThinVec, thin_vec};

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
pub struct Parser<'s> {
    /// The [SourceId] of the source that the parser is currently parsing.
    id: SourceId,

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

    /// The local span map that is used to store created [Span]s for [AstNode]s
    /// that will be synced when the parsing is complete, i.e. at the end of
    /// the `parse_source` query.
    span_map: &'s mut LocalSpanMap,
}

impl<'s> Parser<'s> {
    pub fn new(
        id: SourceId,
        source: SpannedSource<'s>,
        stream: &'s [Token],
        diagnostics: &'s mut ParserDiagnostics,
        span_map: &'s mut LocalSpanMap,
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
            _source: source,
            diagnostics,
            span_map,
            options,
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
    #[inline(always)]
    pub fn node_with_span<T>(&mut self, inner: T, location: ByteRange) -> AstNode<T> {
        let id = self.span_map.add(location);
        AstNode::with_id(inner, id)
    }

    /// Create a new [AstNode] with a span that ranges from the start
    /// [ByteRange] to join with the [ByteRange].
    #[inline(always)]
    pub(crate) fn node_with_joined_span<T>(&mut self, body: T, start: ByteRange) -> AstNode<T> {
        // We get the previous token, before the current since we want to
        // know the span up to the current token, not including it.

        let id = self.span_map.add(start.join(self.previous_pos()));
        AstNode::with_id(body, id)
    }

    /// Create [AstNodes] with a span.
    pub(crate) fn nodes_with_span<T>(
        &mut self,
        nodes: ThinVec<AstNode<T>>,
        location: ByteRange,
    ) -> AstNodes<T> {
        let id = self.span_map.add(location);
        AstNodes::with_id(nodes, id)
    }

    /// Create [AstNodes] with a span that ranges from the start [ByteRange] to
    /// the current [ByteRange].
    pub(crate) fn nodes_with_joined_span<T>(
        &mut self,
        nodes: ThinVec<AstNode<T>>,
        start: ByteRange,
    ) -> AstNodes<T> {
        let id = self.span_map.add(start.join(self.previous_pos()));
        AstNodes::with_id(nodes, id)
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

        ParseError::new(
            kind,
            self.make_span(span.unwrap_or_else(|| self.eof_pos())),
            expected,
            received,
        )
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
    /// line of a `liquid` tag has no delimiters, and so no markers.
    ///
    /// Reference:
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#whitespace-control
    /// - Liquid: https://shopify.github.io/liquid/basics/whitespace/
    /// - Twig: https://twig.symfony.com/doc/3.x/templates.html#whitespace-control
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

    pub fn parse_document(&mut self) -> AstNode<ast::Document> {
        // if self.options.recovery {
        //     log::info!("recovery mode enabled");
        // }

        let start = self.current_pos();
        let mut children = thin_vec![];

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

    fn parse_statement(&mut self) -> ParseResult<Option<AstNode<ast::Statement>>> {
        let token = self.peek().ok_or_else(|| self.make_unexpected_eof())?;
        let statement = match token.kind {
            // For parsing text nodes.
            TokenKind::Text => {
                self.skip_fast(TokenKind::Text); // `<text>` Skip the text token.
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

    fn parse_comment(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        self.skip_fast(TokenKind::Comment); // `<comment>` Skip the comment token.

        Ok(self.node_with_joined_span(ast::Statement::Comment(ast::Comment {}), token.span))
    }

    fn parse_tag(&mut self) -> ParseResult<Option<AstNode<ast::Statement>>> {
        // The last token must be a percent tree, but we ensure this since
        // we pass the responsibility of consuming the header token to then child
        // functions.
        debug_assert!(self.peek().is_some_and(|tok| tok.kind.is_tag_tree()));

        let token = self.peek_raw(1).copied().ok_or_else(|| self.make_unexpected_eof())?;
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
                Keyword::Comment => {
                    self.with_tag_context(TagContext::Comment, |g| g.parse_comment_block())
                }

                kwd if let Some(ctx) = self.tag_context()
                    && ctx.applies_to(kwd) =>
                {
                    return Ok(None);
                }

                // Effectively, special functions that we keep track of.
                Keyword::Extends => self.parse_extends_statement(),
                Keyword::Include => self.parse_include_statement(),
                Keyword::Load => self.parse_load_statement(),
                Keyword::Import => self.parse_import_statement(),
                _ => self.err_with_location(
                    ParseErrorKind::Tag,
                    ExpectedItem::Ident,
                    Some(token.kind),
                    token.span,
                ),
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

    fn parse_variable_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        let (expr, trim) = self.in_tag(Delimiter::Brace, |g| {
            let (subject, subject_span) = g.track_span(|g| {
                if g.options.dialect.variable_supports_operators() {
                    g.parse_compound_expr(0)
                } else {
                    g.parse_expr()
                }
            })?;

            // A name that is followed by anything other than an operator, e.g.
            // `{{ x "a" b=c }}`, is called with the rest as its arguments.
            if !g.exhausted() && subject.body.is_var() {
                // This might be a call expression, try and parse the arguments
                let args = g.parse_args()?;

                return Ok(g.node_with_joined_span(
                    ast::Expr::Call(ast::CallExpr { subject, args }),
                    subject_span,
                ));
            }

            Ok(subject)
        })?;

        Ok(self
            .node_with_joined_span(ast::Statement::Inline(ast::Inline { expr, trim }), token.span))
    }

    fn parse_compound_expr(&mut self, min_precedence: u8) -> ParseResult<AstNode<ast::Expr>> {
        // first of all, we want to get the lhs...
        let (mut lhs, lhs_span) = self.track_span(|this| this.parse_expr())?;

        loop {
            let op_start = self.current_pos();
            // this doesn't consider operators that have an 'eq' variant because that is
            // handled at the statement level, since it isn't really a binary
            // operator...
            let (Some(op), consumed_tokens) = self.parse_bin_op() else {
                break;
            };

            // An operator that the dialect doesn't have, e.g. `*` in Django,
            // doesn't continue the expression. It is left for the tag, which
            // reports it as unexpected, like any token after an expression.
            // Reference:
            // - Django: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
            // - Jinja: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
            // - Liquid: https://shopify.github.io/liquid/basics/operators/#order-of-operations
            // - Twig: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
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
            let rhs = self.parse_compound_expr(r_precedence)?;

            //v transform the operator into an `BinaryExpr`
            let op = self.node_with_span(op, op_span);
            lhs =
                self.node_with_joined_span(ast::Expr::Bin(ast::BinExpr { lhs, rhs, op }), lhs_span);
        }

        Ok(lhs)
    }

    fn parse_expr(&mut self) -> ParseResult<AstNode<ast::Expr>> {
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

    fn parse_expr_component(&mut self, token: Token) -> ParseResult<AstNode<ast::Expr>> {
        // ##Note: Each child path is responsible for skipping the current `token`.
        Ok(match token.kind {
            kind if kind.is_unary_op() => return self.parse_unary_expr(token),

            _ if let Some(lit) = self.liquid_literal(token) => {
                self.skip_fast(token.kind); // `<lit>` Skip the literal token.
                self.node_with_span(ast::Expr::Lit(ast::LitExpr { lit }), token.span)
            }
            // A Liquid range, e.g. `(1..n)`.
            // Reference: https://shopify.github.io/liquid/tags/iteration/#range
            TokenKind::Tree(Delimiter::Paren, _) if self.options.dialect.has_ranges() => {
                return self.parse_range(token);
            }

            kw @ TokenKind::Keyword(keyword) if keyword.identifier_like() => {
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

    /// The Liquid literal that `token` is, if any, e.g. `nil`. In the other
    /// dialects, these are names like any other.
    ///
    /// Reference:
    /// - https://shopify.github.io/liquid/basics/types/#nil
    /// - https://shopify.github.io/liquid/basics/types/#emptydrop
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
    fn parse_range(&mut self, token: Token) -> ParseResult<AstNode<ast::Expr>> {
        let (start, end) = self.in_tree(Delimiter::Paren, None, |g| {
            let start = g.parse_value()?;
            g.parse_token(TokenKind::DotDot)?;
            Ok((start, g.parse_value()?))
        })?;

        Ok(self.node_with_span(ast::Expr::Range(ast::RangeExpr { start, end }), token.span))
    }

    /// Parse a value, i.e. an expression without any filters applied to it.
    fn parse_value(&mut self) -> ParseResult<AstNode<ast::Expr>> {
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
        mut subject: AstNode<ast::Expr>,
        mut subject_span: ByteRange,
    ) -> ParseResult<AstNode<ast::Expr>> {
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
                    let index = self.in_tree(Delimiter::Bracket, None, |g| g.parse_expr())?;

                    self.node_with_joined_span(
                        ast::Expr::Index(ast::IndexExpr { subject, index }),
                        subject_span,
                    )
                }
                // Function call
                // TokenKind::Tree(Delimiter::Paren, _) => self.parse_call(subject, subject_span)?,
                _ => break,
            };

            // We need to adjust the subject_span so we can compute whether
            // we're still "physically" connected to the end of the expression.
            subject_span = subject_span.join(self.previous_pos())
        }

        Ok(subject)
    }

    fn parse_unary_expr(&mut self, token: Token) -> ParseResult<AstNode<ast::Expr>> {
        let op = self.node_with_span(
            match token.kind {
                TokenKind::Keyword(Keyword::Not) => ast::UnaryOp::Not,
                TokenKind::Minus => ast::UnaryOp::Neg,
                _ => unreachable!(),
            },
            token.span,
        );

        self.skip_fast(token.kind); // `<op>` Skip the operator token.

        // Reference:
        // - Django: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
        // - Jinja: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
        // - Liquid: https://shopify.github.io/liquid/basics/operators/#order-of-operations
        // - Twig: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
        let expr = self.parse_compound_expr(self.options.dialect.prefix_binding_power(*op.body))?;
        Ok(self.node_with_joined_span(ast::Expr::Unary(ast::UnaryExpr { op, expr }), token.span))
    }

    fn parse_property_access(
        &mut self,
        subject: AstNode<ast::Expr>,
        subject_span: ByteRange,
    ) -> ParseResult<AstNode<ast::Expr>> {
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

    fn parse_filter(&mut self, subject_span: ByteRange) -> ParseResult<AstNode<ast::Filter>> {
        let name = self.parse_name()?;
        let args = if self.parse_token_fast(TokenKind::Colon).is_some() {
            let start = self.current_pos();
            let mut args = thin_vec![self.parse_filter_arg()?];

            // Liquid filters take a list of arguments, e.g. `f: a, b`.
            // Reference: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
            if self.options.dialect.filter_supports_arg_list() {
                while self.parse_token_fast(TokenKind::Comma).is_some() {
                    args.push(self.parse_filter_arg()?);
                }
            }

            self.nodes_with_joined_span(args, start)
        } else {
            AstNodes::empty(self.make_span(subject_span))
        };

        Ok(self.node_with_joined_span(ast::Filter { name, args }, subject_span))
    }

    /// Parse an argument of a filter, i.e. `a` in `x|f:a`. The argument has no
    /// filters of its own, so in `x|f:a|g` the filter `g` applies to `x|f:a`
    /// rather than to `a`. Liquid filters also take keyword arguments, e.g.
    /// `allow_false: true`.
    ///
    /// Reference: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
    fn parse_filter_arg(&mut self) -> ParseResult<AstNode<ast::Arg>> {
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
        Ok(self.node_with_joined_span(ast::Arg { name, value: Some(value) }, start.span))
    }

    fn parse_bin_op(&mut self) -> (Option<ast::BinOp>, u8) {
        let token = self.peek();

        // check if there is a token that we can peek at ahead...
        if token.is_none() {
            return (None, 0);
        }

        match &(token.unwrap()).kind {
            TokenKind::Keyword(Keyword::Contains) => (Some(ast::BinOp::Contains), 1),
            TokenKind::EqEq => (Some(ast::BinOp::Eq), 1),
            TokenKind::NotEq => (Some(ast::BinOp::NotEq), 1),
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
    fn parse_for_loop(&mut self, kind: ast::LoopKind) -> ParseResult<AstNode<ast::Statement>> {
        let start = self.current_pos();
        let (opening, end) = match kind {
            ast::LoopKind::For => (Keyword::For, Keyword::EndFor),
            ast::LoopKind::TableRow => (Keyword::TableRow, Keyword::EndTableRow),
        };

        // The body for an empty loop comes after `{% empty %}` in Django, and
        // after `{% else %}` in the other dialects.
        // Reference:
        // - Django: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
        // - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#for
        // - Liquid: https://shopify.github.io/liquid/tags/iteration/#else
        // - Twig: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
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
                let iterator = g.parse_expr()?;
                let (reverse_modifier, params) = g.parse_loop_modifiers()?;

                let guard = if g.parse_token_fast(TokenKind::Keyword(Keyword::If)).is_some() {
                    Some(g.parse_expr()?)
                } else {
                    None
                };

                Ok((target, iterator, reverse_modifier, params, guard))
            })?;

        // Next, parse until we reach the end of the loop, or the body for an
        // empty loop.
        let (loop_body, ending_token, ending_trim) = self.parse_body_until_block_footer(
            end,
            |kind| matches!(kind, TokenKind::Keyword(kwd) if kwd == end || Some(kwd) == empty),
            |g| {
                g.skip_token(); // `<empty>` | `<end>` Skip the empty or end token.
                Ok(g.trim())
            },
        )?;

        let (loop_empty, empty_trim, end_trim) = if ending_token != TokenKind::Keyword(end) {
            let (body, _, end_trim) = self.parse_body_until_block_footer(
                end,
                |kind| kind == TokenKind::Keyword(end),
                |g| {
                    g.skip_token(); // `<end>` Skip the end token.
                    Ok(g.trim())
                },
            )?;

            (Some(body), ending_trim, end_trim)
        } else {
            (None, ast::TrimMarker::default(), ending_trim)
        };

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::For(ast::For {
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
            })),
            start,
        ))
    }

    /// Parse the modifiers after the iterator of a loop, in any order: the
    /// `reversed` modifier, and in Liquid, parameters such as `limit: 2`.
    ///
    /// Reference: https://shopify.github.io/liquid/tags/iteration/#for-parameters
    fn parse_loop_modifiers(
        &mut self,
    ) -> ParseResult<(Option<AstNode<ast::Name>>, AstNodes<ast::Arg>)> {
        let mut reverse_modifier = None;
        let mut params = thin_vec![];
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
                        ast::Arg { name: Some(name), value: Some(value) },
                        span,
                    ));
                }
                _ => break,
            }
        }

        Ok((reverse_modifier, self.nodes_with_joined_span(params, start)))
    }

    /// Parse the value of a Liquid loop parameter, e.g. `2` in `limit: 2`.
    fn parse_loop_param_value(&mut self) -> ParseResult<AstNode<ast::Expr>> {
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
    fn parse_for_target(&mut self) -> ParseResult<AstNode<ast::ForTarget>> {
        let start = self.current_pos();
        let key = self.parse_name()?;
        let mut items = thin_vec![key];

        loop {
            if self.parse_token_fast(TokenKind::Comma).is_none() {
                break;
            }

            let key = self.parse_name()?;
            items.push(key);
        }

        let items = self.nodes_with_joined_span(items, start);
        Ok(self.node_with_joined_span(ast::ForTarget { items }, start))
    }

    /// Parse statements up to a `{% %}` tag whose keyword `stop` accepts, or
    /// the end of the input, without parsing that tag.
    fn parse_body_until(
        &mut self,
        stop: impl Fn(Keyword) -> bool,
    ) -> ParseResult<AstNode<ast::Body>> {
        let start = self.current_pos();
        let mut contents = thin_vec![];

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
    fn parse_if_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let mut clauses = thin_vec![];
        let mut otherwise = None;
        let mut else_trim = ast::TrimMarker::default();
        let mut end_trim = None;
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
                        g.parse_compound_expr(0)
                    })?;

                    let clause_kind = match kwd {
                        Keyword::If => bl_ast::ClauseKind::If,
                        Keyword::Unless => bl_ast::ClauseKind::Unless,
                        _ => bl_ast::ClauseKind::Elif,
                    };
                    let clause_body = self.parse_body_until(|kwd| {
                        kwd == end || matches!(kwd, Keyword::Elif | Keyword::Else)
                    })?;
                    clauses.push(self.node_with_joined_span(
                        ast::IfClause { kind: clause_kind, condition, clause_body, trim },
                        start,
                    ));
                }
                TokenKind::Keyword(Keyword::Else) if otherwise.is_none() => {
                    ((), else_trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::Else))
                    })?;

                    otherwise = Some(self.parse_body_until(|kwd| kwd == end)?);
                }
                TokenKind::Keyword(kwd) if kwd == end => {
                    let ((), trim) = self
                        .in_tag(Delimiter::Percent, |g| g.parse_token(TokenKind::Keyword(end)))?;
                    end_trim = Some(trim);
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

        let Some(end_trim) = end_trim else {
            return self.err_with_location(
                ParseErrorKind::UnclosedTag(end),
                ExpectedItem::empty(),
                None,
                self.eof_pos(),
            );
        };

        let clauses = self.nodes_with_joined_span(clauses, start);
        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::If(ast::If { clauses, otherwise, else_trim, end_trim })),
            start,
        ))
    }

    /// Parse a Liquid `case` block, i.e.
    ///
    /// ```liquid
    /// {% case product.type %}
    ///     {% when "shirt", "hat" %} Apparel
    ///     {% else %} Other
    /// {% endcase %}
    /// ```
    fn parse_case_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let start = self.current_pos();
        let is_clause = |kwd| matches!(kwd, Keyword::When | Keyword::Else | Keyword::EndCase);

        let (subject, trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Case))?;
            g.parse_expr()
        })?;

        let leading = self.parse_body_until(is_clause)?;

        let mut branches = thin_vec![];
        let mut otherwise = None;
        let mut else_trim = ast::TrimMarker::default();
        let mut end_trim = None;
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
                        g.parse_when_values()
                    })?;

                    let clause_body = self.parse_body_until(is_clause)?;
                    branches.push(self.node_with_joined_span(
                        ast::When { values, clause_body, trim },
                        clause_start,
                    ));
                }
                TokenKind::Keyword(Keyword::Else) if otherwise.is_none() => {
                    ((), else_trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::Else))
                    })?;

                    otherwise = Some(self.parse_body_until(|kwd| kwd == Keyword::EndCase)?);
                }
                TokenKind::Keyword(Keyword::EndCase) => {
                    let ((), trim) = self.in_tag(Delimiter::Percent, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::EndCase))
                    })?;
                    end_trim = Some(trim);
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

        let Some(end_trim) = end_trim else {
            return self.err_with_location(
                ParseErrorKind::UnclosedTag(Keyword::EndCase),
                ExpectedItem::empty(),
                None,
                self.eof_pos(),
            );
        };

        let branches = self.nodes_with_joined_span(branches, branches_start);
        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Case(ast::Case {
                subject,
                leading,
                branches,
                otherwise,
                trim: ast::TrimTag { start: trim, end: end_trim },
                else_trim,
            })),
            start,
        ))
    }

    /// Parse the values of a `{% when %}` clause, which are separated by `,`
    /// or `or`.
    fn parse_when_values(&mut self) -> ParseResult<AstNodes<ast::Expr>> {
        let start = self.current_pos();
        let mut values = thin_vec![self.parse_expr()?];

        while self
            .parse_token_fast(TokenKind::Comma)
            .or_else(|| self.parse_token_fast(TokenKind::Keyword(Keyword::Or)))
            .is_some()
        {
            values.push(self.parse_expr()?);
        }

        Ok(self.nodes_with_joined_span(values, start))
    }

    fn parse_break_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Break))?;
            Ok(g.node_with_span(ast::Statement::Tag(ast::Tag::Break(ast::Break {})), g.range()))
        })
    }

    fn parse_continue_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Continue))?;
            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Continue(ast::Continue {})),
                g.range(),
            ))
        })
    }

    fn parse_with_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = *self.current_token();

        // Parse the header first, we should get `block <name>`.
        let (assignments, trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::With))?;
            g.parse_assignments()
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

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::With(ast::With {
                assignments,
                block_body,
                kind: ast::AssignmentKind::With,
                trim: ast::TrimTag { start: trim, end: end_trim },
            })),
            token.span,
        ))
    }

    fn parse_block_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = *self.current_token();

        // Parse the header first, we should get `block <name>`.
        let (label, trim) = self.in_tag(Delimiter::Percent, |g| {
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

            g.parse_name_or_string()
        })?;

        // Now parse a bunch of general statements until we reach the end of the block.
        let (block_body, _, (end_label, end_trim)) = self.parse_body_until_block_footer(
            Keyword::EndBlock,
            |kind| matches!(kind, TokenKind::Keyword(Keyword::EndBlock)),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndBlock)); // `<endblock>` Skip the endblock token.

                // @@Todo: check if the name matches the label, if the labels mismatch then
                // we should generate an error.
                let end_label = if g.peek().is_some() { Some(g.parse_name()?) } else { None };
                Ok((end_label, g.trim()))
            },
        )?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Block(ast::Block {
                label: Some(label),
                block_body,
                end_label,
                trim: ast::TrimTag { start: trim, end: end_trim },
            })),
            token.span,
        ))
    }

    fn parse_raw_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
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
                Ok(())
            },
        )?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Raw(ast::Raw { block_body })),
            start,
        ))
    }

    fn parse_comment_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let start = self.current_pos();

        while let Some(token) = self.peek() {
            // If it's a percent tree (and of length 1), then we can check if it's the end
            // of the block.
            if token.kind.is_tag_tree() {
                let maybe_token = self.peek_raw(1).map(|tok| tok.kind);
                if let Some(TokenKind::Keyword(Keyword::EndComment)) = maybe_token {
                    self.in_tree(Delimiter::Percent, None, |g| {
                        g.parse_token(TokenKind::Keyword(Keyword::EndComment))?;
                        Ok(())
                    })?;
                    break;
                }
            }

            self.skip_token();
        }

        Ok(self.node_with_joined_span(ast::Statement::Comment(ast::Comment {}), start))
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
    ) -> ParseResult<(AstNode<ast::Body>, TokenKind, U)> {
        let mut statements = thin_vec![];
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

    fn parse_args(&mut self) -> ParseResult<AstNodes<ast::Arg>> {
        let mut args = thin_vec![];
        let start = self.current_pos();

        while self.peek().is_some() {
            // Liquid separates a tag's arguments with commas.
            // Reference:
            // - https://shopify.github.io/liquid/tags/iteration/#cycle
            // - https://shopify.github.io/liquid/tags/template/#render-parameters
            if self.options.dialect.requires_comma_separated_args() {
                self.parse_token_fast(TokenKind::Comma);
            }

            match self.parse_arg() {
                Ok(Some(arg)) => args.push(arg),
                Ok(None) => break,
                Err(err) => return Err(err),
            }
        }

        Ok(self.nodes_with_joined_span(args, start))
    }

    /// Parse a Liquid `render` tag, i.e.
    ///
    /// ```liquid
    /// {% render "card", product: product %}
    /// {% render "card" with featured as product %}
    /// {% render "card" for products as product %}
    /// ```
    fn parse_render_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
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

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Render(ast::Render {
                    template,
                    with_value,
                    for_value,
                    alias,
                    args,
                })),
                g.range(),
            ))
        })
    }

    /// Parse a Liquid `{% liquid %}` tag. The lexer gives each of its lines a
    /// `{% %}` tree, so the lines are parsed like any other tags, up to the
    /// tree for its `%}`.
    fn parse_liquid_tag(&mut self) -> ParseResult<AstNode<ast::Statement>> {
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
    fn parse_capture_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = *self.current_token();

        let (name, trim) = self.in_tag(Delimiter::Percent, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Capture))?;

            g.parse_name_or_string()
        })?;

        let (block_body, _, end_trim) = self.parse_body_until_block_footer(
            Keyword::EndCapture,
            |kind| kind == TokenKind::Keyword(Keyword::EndCapture),
            |g| {
                g.skip_fast(TokenKind::Keyword(Keyword::EndCapture)); // `<endcapture>` Skip the end token.
                Ok(g.trim())
            },
        )?;

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::Capture(ast::Capture {
                name,
                block_body,
                trim: ast::TrimTag { start: trim, end: end_trim },
            })),
            token.span,
        ))
    }

    /// Parse an `extends` statement, i.e.
    ///
    ///
    /// ```html
    /// {% extends "base.html" %}
    /// ```
    ///
    /// Based on: https://www.w3schools.com/django/django_tags_extends.php
    fn parse_extends_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Extends))?;
            let template = g.parse_expr()?;

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Extends(ast::Extends { template, trim: g.trim() })),
                g.range(),
            ))
        })
    }

    /// Parse an `include` statement, i.e.
    ///
    /// ```html
    /// {% include "header.html" with title="Header" %}
    /// ```
    ///
    /// Based on: https://www.w3schools.com/django/django_tags_include.php
    fn parse_include_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        // Parse the header first, we should get `block <name>`.
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Include))?;
            let template = g.parse_expr()?;

            // With comma separated arguments, the comma after the template
            // starts them rather than `with`, e.g. `{% include "card", product: product
            // %}`.
            // Reference: https://shopify.github.io/liquid/tags/template/#render-parameters
            let context = if g.parse_token_fast(TokenKind::Keyword(Keyword::With)).is_some()
                || g.options.dialect.requires_comma_separated_args()
            {
                g.parse_args()?
            } else {
                g.nodes_with_span(thin_vec![], g.range())
            };

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Include(ast::Include { template, context })),
                g.range(),
            ))
        })
    }

    fn parse_load_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
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

    /// Parse a Jinja import statement, i.e.
    ///
    /// ```html
    /// {% import "forms.html" as forms %}
    /// ```
    ///
    /// Reference: https://jinja.palletsprojects.com/en/stable/templates/#import
    fn parse_import_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(Keyword::Import))?;
            let template = g.parse_string()?;

            let names = if g.parse_token_fast(TokenKind::Keyword(Keyword::As)).is_some() {
                g.parse_names()?
            } else {
                // This is effectively a dummy range, since we don't have an alias.
                g.nodes_with_span(thin_vec![], g.range())
            };

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Import(ast::Import { template, names })),
                g.range(),
            ))
        })
    }

    fn parse_names(&mut self) -> ParseResult<AstNodes<ast::Name>> {
        let mut names = thin_vec![];
        let start = self.current_pos();

        while self.peek().is_some() {
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
    fn parse_name_or_string(&mut self) -> ParseResult<AstNode<ast::Name>> {
        match self.peek().copied() {
            Some(Token { kind: TokenKind::Str, span }) => {
                self.skip_fast(TokenKind::Str); // `<string>` Skip the string token.
                Ok(self.node_with_span(ast::Name::new(ast::Identifier::from(0u32)), span))
            }
            _ => self.parse_name(),
        }
    }

    fn parse_name(&mut self) -> ParseResult<AstNode<ast::Name>> {
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

    fn parse_string(&mut self) -> ParseResult<AstNode<ast::StrLit>> {
        match self.peek() {
            Some(Token { kind: TokenKind::Str, span }) => {
                self.skip_fast(TokenKind::Str); // `<string>` Skip the string token.

                Ok(self.node_with_span(ast::StrLit {}, *span))
            }
            token => self.err_with_location(
                ParseErrorKind::UnExpected,
                ExpectedItem::Literal,
                token.map(|tok| tok.kind),
                self.expected_pos(),
            ),
        }
    }

    fn parse_arg(&mut self) -> ParseResult<Option<AstNode<ast::Arg>>> {
        match (self.peek().copied(), self.peek_second().copied()) {
            (Some(Token { kind, span }), Some(tok!(Eq))) if kind.is_ident_like() => {
                let name = self.parse_name()?;
                self.parse_token(TokenKind::Eq)?;

                let value = self.parse_expr()?;

                Ok(Some(self.node_with_joined_span(
                    ast::Arg { name: Some(name), value: Some(value) },
                    span,
                )))
            }
            // The name can also be a string, e.g. the group of a Liquid `cycle`
            // in `{% cycle "group": "a", "b" %}`.
            // Reference:
            // - https://shopify.github.io/liquid/tags/iteration/#cycle
            // - https://shopify.github.io/liquid/tags/template/#render-parameters
            (Some(Token { kind, span }), Some(tok!(Colon)))
                if self.options.dialect.named_args_use_colon()
                    && (kind.is_ident_like() || kind == TokenKind::Str) =>
            {
                let name = self.parse_name_or_string()?;

                self.skip_fast(TokenKind::Colon); // `:` Skip the colon token.
                let value = self.parse_expr()?;

                Ok(Some(self.node_with_joined_span(
                    ast::Arg { name: Some(name), value: Some(value) },
                    span,
                )))
            }
            (Some(Token { kind, span }), _) if kind.starts_expr() => {
                let value = self.parse_expr()?;
                Ok(Some(
                    self.node_with_joined_span(ast::Arg { name: None, value: Some(value) }, span),
                ))
            }
            _ => Ok(None),
        }
    }

    fn parse_assignments(&mut self) -> ParseResult<AstNodes<ast::Assignment>> {
        let mut assignments = thin_vec![];
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
        }

        Ok(self.nodes_with_joined_span(assignments, start))
    }

    fn parse_assignment(&mut self) -> ParseResult<Option<AstNode<ast::Assignment>>> {
        let position = self.cursor.position();
        let start = self.current_pos();

        match (self.peek(), self.peek_second()) {
            (Some(tok!(Ident)), Some(tok!(Eq))) => {
                let name = self.parse_name()?;
                self.skip_fast(TokenKind::Eq); // `<eq>` Skip the assignment operator token.
                let value = self.parse_expr()?;
                Ok(Some(self.node_with_joined_span(ast::Assignment { name, value }, start)))
            }
            // Django's `value as name`, e.g. `{% with business.employees.count as total %}`.
            // Reference: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#with
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
