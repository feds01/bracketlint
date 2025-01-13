//! The parser module is responsible for converting a stream of tokens into an
//! abstract syntax tree.
//!
//! Based on several documentation sources:
//!
//! - https://jinja.palletsprojects.com/en/stable/templates/

use std::cell::Cell;

use bl_ast::{
    self as ast, AstNode, AstNodes, ByteRange, Identifier, LocalSpanMap, SourceId, Span,
    SpannedSource, VarExpr,
};
use bl_lexer::token::{
    self, Delimiter, Keyword, NumberFlags, Token, TokenKind, cursor::TokenCursor,
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


#[derive(Deref)]
pub struct ParseFrame<'s> {
    /// The token cursor for the current frame.
    #[deref]
    cursor: TokenCursor<'s>,

    /// If the current frame has an error.
    error: Cell<bool>,
}

impl<'s> ParseFrame<'s> {
    pub fn from_stream(stream: &'s [Token], span: ByteRange) -> Self {
        Self { error: Cell::new(false), cursor: TokenCursor::new(stream, span) }
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
    pub(crate) fn current_pos(&self) -> ByteRange {
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

    /// Check whether the frame has encountered an error.
    #[inline(always)]
    pub(crate) fn has_error(&self) -> bool {
        self.error.get()
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
    /// ```
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
    pub fn _note_on_span(&self, span: ByteRange, note: impl Into<String>) {
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

        let children = self.nodes_with_joined_span(children, start);
        self.node_with_joined_span(ast::Document { children }, start)
    }

    fn parse_statement(&mut self) -> ParseResult<Option<AstNode<ast::Statement>>> {
        let token = self.peek().ok_or_else(|| self.make_unexpected_eof())?;
        let statement = match token.kind {
            _ => self.err_with_location(
                ParseErrorKind::Statement,
                ExpectedItem::empty(),
                None,
                self.expected_pos(),
            ),
        }?;

        Ok(Some(statement))
    }
    fn parse_lit(&self) -> ParseResult<ast::Lit> {
        let token = self.current_token();

        match token.kind {
            TokenKind::Keyword(token::Keyword::False) => {
                self.skip_fast(TokenKind::Keyword(token::Keyword::False)); // `<false>` Skip the false token.
                Ok(ast::Lit::Bool(ast::BoolLit { value: false }))
            }
            TokenKind::Keyword(token::Keyword::True) => {
                self.skip_fast(TokenKind::Keyword(token::Keyword::True)); // `<true>` Skip the true token.
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
    }
}
