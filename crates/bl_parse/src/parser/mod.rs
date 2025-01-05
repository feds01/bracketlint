//! The parser module is responsible for converting a stream of tokens into an
//! abstract syntax tree.
#![allow(dead_code)]

use std::cell::Cell;

use bl_ast::{AstNode, AstNodes, ByteRange, Document, LocalSpanMap, SpannedSource};
use bl_lexer::token::{cursor::TokenCursor, Token};
use derive_more::Deref;
use thin_vec::{thin_vec, ThinVec};

use crate::{diagnostics::ParserDiagnostics, ParseOptions};

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
    source: SpannedSource<'s>,

    stream: &'s [Token],

    #[deref]
    frame: ParseFrame<'s>,

    /// Collected diagnostics for the current [Parser].
    pub(crate) diagnostics: &'s mut ParserDiagnostics,

    /// Any options that are supplied to the parser to alter
    /// its mode of operation.
    options: ParseOptions,

    span_map: &'s mut LocalSpanMap,
}

impl<'s> Parser<'s> {
    pub fn new(
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
            source,
            stream,
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

    pub fn node_with_joined_span<N>(&mut self, body: N, start: ByteRange) -> AstNode<N> {
        // We get the previous token, before the current since we want to
        // know the span up to the current token, not including it.

        let id = self.span_map.add(start.join(self.previous_pos()));
        AstNode::with_id(body, id)
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

    pub fn parse_document(&mut self) -> AstNode<Document> {
        // if self.options.recovery {
        //     log::info!("recovery mode enabled");
        // }

        let start = self.current_pos();
        let children = thin_vec![];

        let children = self.nodes_with_joined_span(children, start);
        self.node_with_joined_span(Document { children }, start)
    }
}
