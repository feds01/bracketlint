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
}
impl TagContext {
    fn applies_to(&self, kwd: token::Keyword) -> bool {
        match self {
            TagContext::If => matches!(kwd, token::Keyword::Else | token::Keyword::Elif),
            TagContext::Block => matches!(kwd, token::Keyword::EndBlock),
            TagContext::With => matches!(kwd, token::Keyword::EndWith),
            TagContext::Comment => matches!(kwd, token::Keyword::EndComment),
            TagContext::Raw => matches!(kwd, token::Keyword::EndRaw),
            TagContext::For => matches!(kwd, token::Keyword::EndFor | token::Keyword::Empty),
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
}

impl<'s> ParseFrame<'s> {
    pub fn from_stream(stream: &'s [Token], span: ByteRange) -> Self {
        Self { error: Cell::new(false), cursor: TokenCursor::new(stream, span), tag_context: None }
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
            // For parsing `{% ... %}` blocks.
            TokenKind::Tree(Delimiter::Percent, _) => return self.parse_tag(),
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
        debug_assert!(self.peek().is_some_and(|tok| tok.kind.is_percent_tree()));

        let token = self.peek_raw(1).copied().ok_or_else(|| self.make_unexpected_eof())?;
        let statement = match token.kind {
            TokenKind::Keyword(keyword) => match keyword {
                // Key control flow components, once that imply a more complex structure
                // of subsequent tags.
                token::Keyword::For => {
                    self.with_tag_context(TagContext::For, |g| g.parse_for_loop())
                }

                token::Keyword::If => self.with_tag_context(TagContext::If, |g| g.parse_if_block()),

                // Control flow tags, that are effectively standalone.
                token::Keyword::Break => self.parse_break_statement(),
                token::Keyword::Continue => self.parse_continue_statement(),
                kwd if let Some(ctx) = self.tag_context()
                    && ctx.applies_to(kwd) =>
                {
                    return Ok(None);
                }

                // Effectively, special functions that we keep track of.
                token::Keyword::Extends => self.parse_extends_statement(),
                token::Keyword::Include => self.parse_include_statement(),
                token::Keyword::Import => self.parse_import_statement(),
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
        let expr = self.in_tree(Delimiter::Brace, None, |g| {
            let subject = g.parse_expr()?;

            // If the subject is an identifier, we ha
            if !g.exhausted() && subject.body.is_var() {
                // This might be a call expression, try and parse the arguments
                let args = g.parse_args()?;

                return Ok(g.node_with_joined_span(
                    ast::Expr::Call(ast::CallExpr { subject, args }),
                    token.span,
                ));
            }

            Ok(subject)
        })?;

        Ok(self.node_with_joined_span(ast::Statement::Inline(ast::Inline { expr }), token.span))
    }

    fn parse_compound_expr(&mut self, min_prec: u8) -> ParseResult<AstNode<ast::Expr>> {
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

            // check if we have higher precedence than the lhs expression...
            let (l_prec, r_prec) = op.infix_binding_power();

            if l_prec < min_prec {
                break;
            }

            // Now skip the consumed tokens...
            self.skip(consumed_tokens);

            let op_span = op_start.join(self.current_pos());
            let rhs = self.parse_compound_expr(r_prec)?;

            //v transform the operator into an `BinaryExpr`
            let op = self.node_with_span(op, op_span);
            lhs =
                self.node_with_joined_span(ast::Expr::Bin(ast::BinExpr { lhs, rhs, op }), lhs_span);
        }

        Ok(lhs)
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

    fn parse_bin_op(&mut self) -> (Option<ast::BinOp>, u8) {
        let token = self.peek();

        // check if there is a token that we can peek at ahead...
        if token.is_none() {
            return (None, 0);
        }

        match &(token.unwrap()).kind {
            TokenKind::EqEq => (Some(ast::BinOp::Eq), 1),
            TokenKind::NotEq => (Some(ast::BinOp::NotEq), 1),
            TokenKind::Lt => (Some(ast::BinOp::Lt), 1),
            TokenKind::LtEq => (Some(ast::BinOp::LtEq), 1),
            TokenKind::Gt => (Some(ast::BinOp::Gt), 1),
            TokenKind::GtEq => (Some(ast::BinOp::GtEq), 1),

            TokenKind::Keyword(token::Keyword::And) => (Some(ast::BinOp::And), 1),
            TokenKind::Keyword(token::Keyword::Or) => (Some(ast::BinOp::Or), 1),
            TokenKind::Keyword(token::Keyword::In) => (Some(ast::BinOp::In), 1),
            TokenKind::Keyword(token::Keyword::Is) => match self.peek_second() {
                Some(Token { kind: TokenKind::Keyword(token::Keyword::Not), .. }) => {
                    (Some(ast::BinOp::NotEq), 2)
                }
                _ => (Some(ast::BinOp::Is), 1),
            },
            TokenKind::Keyword(token::Keyword::Not) => match self.peek_second() {
                Some(Token { kind: TokenKind::Keyword(token::Keyword::In), .. }) => {
                    (Some(ast::BinOp::NotEq), 2)
                }
                _ => (None, 0),
            },
            _ => (None, 0),
        }
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

    fn parse_for_loop(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let start = self.current_pos();

        // Parse the header first, which is within the current token.
        let (target, iterator, reverse_modifier, guard) =
            self.in_tree(Delimiter::Percent, None, |g| {
                g.parse_token(TokenKind::Keyword(token::Keyword::For))?;
                let target = g.parse_for_target()?;

                g.parse_token(TokenKind::Keyword(token::Keyword::In))?;
                let iterator = g.parse_expr()?;

                let reverse_modifier =
                    if g.parse_token_fast(TokenKind::Keyword(token::Keyword::Reversed)).is_some() {
                        Some(g.node_with_span(
                            ast::Name::new(ast::Identifier::from(0u32)),
                            g.previous_pos(),
                        ))
                    } else {
                        None
                    };

                if g.parse_token_fast(TokenKind::Keyword(token::Keyword::If)).is_some() {
                    let guard = g.parse_expr()?;
                    Ok((target, iterator, reverse_modifier, Some(guard)))
                } else {
                    Ok((target, iterator, reverse_modifier, None))
                }
            })?;

        // Next, parse either until we reach an `endfor` or `empty` token.
        //
        // If it's an empty token, then we reach the end of the loop.
        let (loop_body, ending_token) = self.parse_body_until_block_footer(
            ExpectedItem::empty(),
            |kind| {
                matches!(kind, TokenKind::Keyword(token::Keyword::EndFor | token::Keyword::Empty))
            },
            |g| {
                g.skip_token(); // `<empty>` | `<end_for>` Skip the empty token.
                Ok(())
            },
        )?;

        // If we ended with an `{% empty %}` token, then we parse the empty block.
        let loop_empty = if ending_token == TokenKind::Keyword(token::Keyword::Empty) {
            let (body, _) = self.parse_body_until_block_footer(
                ExpectedItem::empty(),
                |kind| matches!(kind, TokenKind::Keyword(token::Keyword::EndFor)),
                |g| {
                    g.skip_token(); // `<end_for>` Skip the empty token.
                    Ok(())
                },
            )?;

            Some(body)
        } else {
            None
        };

        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::For(ast::For {
                target,
                iterator,
                guard,
                loop_body,
                loop_empty,
                reverse_modifier,
            })),
            start,
        ))
    }

    /// Parse the destructuring target of a for loop, i.e. the assigned variable
    /// or variables per iteration, e.g.
    /// ```
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

    fn parse_if_block(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        let mut clauses = thin_vec![];
        let mut otherwise = None;
        let start = self.current_pos();

        let parse_body = |this: &mut Self, preceding_token: TokenKind| {
            let start = this.current_pos();
            let mut contents = thin_vec![];

            while let Some(token) = this.peek() {
                if token.kind.is_percent_tree() {
                    let maybe_token = this.peek_raw(1).map(|t| t.kind);
                    if let Some(kind) = maybe_token
                        && kind.is_control_flow_for(preceding_token)
                    {
                        break;
                    }
                }

                if let (Some(statement), _) = this.track_span(|g| g.parse_statement())? {
                    contents.push(statement);
                } else {
                    break;
                }
            }

            let contents = this.nodes_with_joined_span(contents, start);
            Ok(this.node_with_joined_span(ast::Body { contents }, start))
        };

        while let Some(token) = self.peek() {
            // If it's a percent tree (and of length 1), then we can check if it's the end
            // of the block.
            if let TokenKind::Tree(Delimiter::Percent, len) = token.kind
                && len > 0
            {
                let token = self.peek_raw(1).copied().unwrap();

                match token {
                    Token { kind: TokenKind::Keyword(Keyword::If | Keyword::Elif), .. } => {
                        let condition = self.in_tree(Delimiter::Percent, None, |g| {
                            g.skip_token();
                            g.parse_compound_expr(0)
                        })?;

                        let if_body = parse_body(self, token.kind)?;
                        clauses.push(
                            self.node_with_joined_span(ast::IfClause { condition, if_body }, start),
                        );
                    }
                    Token { kind: TokenKind::Keyword(Keyword::Else), .. } => {
                        self.in_tree(Delimiter::Percent, None, |g| {
                            g.parse_token(TokenKind::Keyword(Keyword::Else))?;
                            Ok(())
                        })?;

                        otherwise = Some(parse_body(self, token.kind)?);
                    }
                    Token { kind: TokenKind::Keyword(Keyword::EndIf), .. } => {
                        self.in_tree(Delimiter::Percent, None, |g| {
                            g.parse_token(TokenKind::Keyword(Keyword::EndIf))?;
                            Ok(())
                        })?;
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
        }

        let clauses = self.nodes_with_joined_span(clauses, start);
        Ok(self.node_with_joined_span(
            ast::Statement::Tag(ast::Tag::If(ast::If { clauses, otherwise })),
            start,
        ))
    }

    fn parse_break_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(token::Keyword::Break))?;
            Ok(g.node_with_span(ast::Statement::Tag(ast::Tag::Break(ast::Break {})), g.range()))
        })
    }

    fn parse_continue_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(token::Keyword::Continue))?;
            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Continue(ast::Continue {})),
                g.range(),
            ))
        })
    }

    fn parse_body_until_block_footer<U>(
        &mut self,
        expected: ExpectedItem,
        peek_fn: impl Fn(TokenKind) -> bool,
        g: impl FnMut(&mut Self) -> ParseResult<U>,
    ) -> ParseResult<(AstNode<ast::Body>, TokenKind)> {
        let mut statements = thin_vec![];
        let start = self.current_pos();
        let mut end: Option<_> = None;

        while let Some(token) = self.peek() {
            // If it's a percent tree (and of length 1), then we can check if it's the end
            // of the block.
            if let TokenKind::Tree(Delimiter::Percent, _) = token.kind {
                let maybe_token = self.peek_raw(1);
                if let Some(inner) = maybe_token
                    && peek_fn(inner.kind)
                {
                    // Parse the end of the block now and record the span.
                    end = Some(*inner);
                    self.in_tree(Delimiter::Percent, None, g)?;
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
        if let Some(end) = end {
            let span = start.join(end.span);
            let contents = self.nodes_with_span(statements, span);
            Ok((self.node_with_joined_span(ast::Body { contents }, span), end.kind))
        } else {
            self.err_with_location(ParseErrorKind::UnclosedTag, expected, None, self.eof_pos())
        }
    }

    fn parse_args(&mut self) -> ParseResult<AstNodes<ast::Arg>> {
        let mut args = thin_vec![];
        let start = self.current_pos();

        while self.peek().is_some() {
            match self.parse_arg() {
                Ok(Some(arg)) => args.push(arg),
                Ok(None) => break,
                Err(err) => return Err(err),
            }
        }

        Ok(self.nodes_with_joined_span(args, start))
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
            g.parse_token(TokenKind::Keyword(token::Keyword::Extends))?;
            let template = g.parse_expr()?;

            Ok(g.node_with_span(
                ast::Statement::Tag(ast::Tag::Extends(ast::Extends { template })),
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
            g.parse_token(TokenKind::Keyword(token::Keyword::Include))?;
            let template = g.parse_expr()?;

            let context = if g.parse_token_fast(TokenKind::Keyword(token::Keyword::With)).is_some()
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

    /// Parse a Jinja import statement, i.e.
    ///
    /// ```html
    /// {% import "forms.html" as forms %}
    /// ```
    ///
    /// Reference: https://jinja.palletsprojects.com/en/stable/templates/#import
    fn parse_import_statement(&mut self) -> ParseResult<AstNode<ast::Statement>> {
        self.in_tree(Delimiter::Percent, None, |g| {
            g.parse_token(TokenKind::Keyword(token::Keyword::Import))?;
            let template = g.parse_string()?;

            let names = if g.parse_token_fast(TokenKind::Keyword(token::Keyword::As)).is_some() {
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
            (
                Some(Token { kind: TokenKind::Ident, span }),
                Some(Token { kind: TokenKind::Eq, .. }),
            ) => {
                let name = self.parse_name()?;
                self.parse_token(TokenKind::Eq)?;

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
