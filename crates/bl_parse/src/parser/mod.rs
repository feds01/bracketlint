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

    /// Create new AST generator from a provided token stream with inherited
    /// module resolver and a provided parent span.
    fn new_frame<T>(
        &mut self,
        start: usize,
        len: usize,
        parent_span: ByteRange,
        mut g: impl FnMut(&mut Self) -> T,
    ) -> T {
        let new_frame =
            ParseFrame::from_stream(&self.frame.stream()[start..(start + len)], parent_span);
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
            Some(Token { kind: TokenKind::Tree(inner, len), span }) if *inner == delimiter => {
                // The start of the tree is the actual `Tree` token, and then we slice
                // from it up to the specified `len` of the tree.
                let start = self.position() + 1;

                self.skip_token(); // We want to update our position, when we return to this generator.
                self.new_frame(start, *len as usize, *span, g)
            }
            token => self.err_with_location(
                error.unwrap_or(ParseErrorKind::UnExpected),
                ExpectedItem::from(delimiter),
                token.map(|tok| tok.kind),
                token.map_or_else(|| self.current_pos(), |tok| tok.span),
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
            // For parsing text nodes.
            TokenKind::Text => {
                self.skip_fast(TokenKind::Text); // `<text>` Skip the text token.
                Ok(self.node_with_span(ast::Statement::text(), token.span))
            }
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


    fn parse_variable_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;
        let expr = self.in_tree(Delimiter::Brace, None, |g| {
            let subject = g.parse_expr()?;

            Ok(subject)
        })?;

        Ok(self.node_with_joined_span(ast::Statement::Inline(ast::Inline { expr }), token.span))
    }


    fn parse_expr(&mut self) -> ParseResult<AstNode<ast::Expr>> {
        let token = self.peek().copied().ok_or_else(|| self.make_unexpected_eof())?;

        // Firstly, we have to get the initial part of the expression,
        // and then we can check if there are any additional parts in the
        // forms of either property accesses, indexing or method calls
        let (subject, subject_span) = self.track_span(|this| this.parse_expr_component(token))?;

        self.parse_singular_expr(subject, subject_span)
    }

    fn parse_expr_component(&mut self, token: Token) -> ParseResult<AstNode<ast::Expr>> {
        // ##Note: Each child path is responsible for skipping the current `token`.
        Ok(match token.kind {
            kind if kind.is_unary_op() => return self.parse_unary_expr(token),

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

    /// Provided an initial subject expression that is parsed by the parent
    /// caller, this function will check if there are any additional
    /// components to the expression; in the form of either property access,
    /// method calls, indexing, etc.
    pub(crate) fn parse_singular_expr(
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
                    let span = token.span;
                    let index = self.in_tree(Delimiter::Bracket, None, |g| g.parse_expr())?;

                    self.node_with_joined_span(
                        ast::Expr::Index(ast::IndexExpr { subject, index }),
                        span,
                    )
                }
                // Filter
                TokenKind::Pipe => {
                    self.skip_fast(TokenKind::Pipe); // `<pipe>` Skip the pipe token.
                    let filter = self.parse_filter(subject_span)?;
                    self.node_with_joined_span(
                        ast::Expr::FilteredExpr(ast::FilteredExpr { subject, filter }),
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
                TokenKind::Keyword(token::Keyword::Not) => ast::UnaryOp::Not,
                TokenKind::Minus => ast::UnaryOp::Neg,
                _ => unreachable!(),
            },
            token.span,
        );

        self.skip_fast(token.kind); // `<op>` Skip the operator token.
        let expr = self.parse_expr()?;
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
            self.parse_args()?
        } else {
            AstNodes::empty(self.make_span(subject_span))
        };

        Ok(self.node_with_joined_span(ast::Filter { name, args }, subject_span))
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
