//! Implementation of [TokenCursor] which enables the compiler to iterate
//! over tokens whilst hiding away the specifics of the underlying token
//! stream.
//!
//! To give a further explanation for the need of a [TokenCursor], we look
//! at the following code snippet and the corresponding token stream:
//! ```ignore
//! if x { 2 + 2 } else { 3 + 3 }
//! ```
//!
//! The snippet above will be tokenised by the [Lexer] into the following:
//! ```ignore
//! kw    "if"
//! ident "x"
//! tree  "{" 3 <--- // Start of tree.
//! int   "2"    |   // All of the tokens of the tree are put in contiguous memory
//! plus         |   // after the initial tree token which denotes the length of the
//! int   "2"    |   // tree.
//! kw    "else"
//! tree  "{" 3 <--- // Start of second tree
//! int   "3"    |
//! plus         |
//! int   "3"    |
//! ```
//!
//! Nested trees are handled in the same way, here is an example which
//! is annotated with token counts beginning from each token tree within
//! the stream:
//! ```
//! if x { if y { 2 } print( "bing") }
//!      1  2 3 4 5   6    7 8  // `{`
//!               1             // `{`
//!                        1    // `(`
//! ```
//!
//! Notice that the closing delimiter is not counted in the tokens. This is
//! because the closing delimiter is not needed to determine the length of
//! the tree.
//!
//! Due to this structure in the tokens, the parser and other parts of the
//! compiler can use [TokenCursor] to iterate through tokens skipping over trees
//! as if they were single tokens.

use std::cell::Cell;

use bl_ast::ByteRange;

use crate::token::{Token, TokenKind};

pub type TokenStream<'t> = &'t [Token];

/// A cursor over a token stream which handles internal traversal logic
/// and bookkeeping.
pub struct TokenCursor<'t> {
    /// The raw stream of tokens that is being traversed.
    stream: TokenStream<'t>,

    /// The [ByteRange] of the previous `tree` if the previously skipped
    /// token was a tree.
    prev_tree: Cell<Option<ByteRange>>,

    /// The [ByteRange] of the the stream, this covers the entire [TokenStream].
    span: ByteRange,

    /// The current position of the cursor.
    pos: Cell<usize>,
}

impl<'t> TokenCursor<'t> {
    /// Create a new [TokenCursor].
    pub fn new(stream: TokenStream<'t>, span: ByteRange) -> Self {
        Self { stream, span, pos: Cell::new(0), prev_tree: Cell::new(None) }
    }

    /// Get the [TokenStream].
    #[inline(always)]
    pub fn stream(&self) -> TokenStream<'t> {
        self.stream
    }

    /// Get the current offset of where the stream is at.
    #[inline(always)]
    pub fn position(&self) -> usize {
        self.pos.get()
    }

    /// Get the previous token.
    #[inline(always)]
    pub fn previous_token(&self) -> &Token {
        self.stream.get(self.pos.get().wrapping_sub(1)).unwrap()
    }

    /// Get the current token.
    #[inline(always)]
    pub fn current_token(&self) -> &Token {
        self.stream.get(self.pos.get()).unwrap()
    }

    /// Get the current token, and advance to the next token if there
    /// exists another token in the [TokenStream].
    #[inline(always)]
    pub fn current_token_and_advance(&self) -> Option<&Token> {
        // Look at the current token, and check if we need
        // skip over a tree...
        let value = self.stream.get(self.pos.get())?;
        self.skip_token();

        Some(value)
    }

    /// Skip `n` tokens ahead.
    ///
    /// # Safety
    ///
    /// ##Warning This is a potentially unsafe operation because it doesn't
    /// perform any checking on a tree range. Only use this function when you
    /// are sure that
    #[inline(always)]
    pub unsafe fn skip(&self, n: u8) {
        self.pos.set(self.pos.get() + n as usize);
        self.prev_tree.set(None);
    }

    pub fn skip_token(&self) {
        if let Some(Token { kind: TokenKind::Tree(_, size), span }) = self.peek().copied() {
            self.prev_tree.set(Some(span));
            self.pos.set(self.pos.get() + size as usize + 1)
        } else {
            self.prev_tree.set(None);
            self.pos.set(self.pos.get() + 1)
        }
    }

    /// Skip the current token.
    #[inline(always)]
    pub fn skip_fast(&self, kind: TokenKind) {
        debug_assert!(!matches!(self.peek_kind(), Some(TokenKind::Tree(_, _))));
        debug_assert!(matches!(self.peek_kind(), Some(k) if k == kind));

        self.prev_tree.set(None);
        self.pos.set(self.pos.get() + 1)
    }

    pub fn previous_pos(&self) -> ByteRange {
        if let Some(tree) = self.prev_tree.get() {
            return tree;
        }

        // If we don't have a tree, the stream is empty, or we have gone
        // beyond the range, we just return the range of the current token.
        if self.is_empty() || self.position() > self.len() {
            return self.span;
        }

        // Otherwise, we read the span of the previous token.
        let val = self.position().saturating_sub(1);
        unsafe { self.stream.get_unchecked(val).span }
    }

    /// Set the position of the `token` cursor.
    ///
    /// # Safety
    ///
    /// ##Warning: This is a potentially unsafe operation because it doesn't
    /// perform any checking on tree ranges.
    #[inline(always)]
    pub unsafe fn set_pos(&self, pos: usize) {
        self.pos.set(pos);
        self.prev_tree.set(None);
    }

    /// Attempt to peek one step token ahead.
    #[inline(always)]
    pub fn peek(&self) -> Option<&Token> {
        self.stream.get(self.pos.get())
    }

    /// Attempt to peek at the next [TokenKind].
    #[inline(always)]
    pub fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|tok| tok.kind)
    }

    /// Peek two tokens ahead.
    #[inline(always)]
    pub fn peek_second(&self) -> Option<&Token> {
        let first = self.stream.get(self.pos.get())?;

        // If the first is a tree, we have to jump over it...
        if let TokenKind::Tree(_, size) = first.kind {
            self.stream.get(self.pos.get() + size as usize + 1)
        } else {
            self.stream.get(self.pos.get() + 1)
        }
    }

    /// Lookup a token within the flat token stream instead of adhering
    /// to the tree structure. This is useful for when we need to disambiguate
    /// on how to parse a particular token tree. @@Future: Perhaps later we can
    /// instead use `peek_tree()`, and then perform some kind of peek within
    /// the tree itself to determine on how to parse the tree.
    ///
    /// ## Safety
    ///
    /// > Use this function with caution as it doesn't perform any checks
    /// > on the tree ranges.
    ///
    /// ```
    /// # use bl_lexer::token::{Token, TokenKind};
    ///
    /// let token = self.peek(1).unwrap_or_else(SomeErr(..))?;
    /// assert!(token.kind.is_percent_tree());
    ///
    /// // Suppose that we wanna check the next token within the tree:
    ///
    /// let next = self.peek_second().unwrap_or_else(SomeErr(..))?; // 💣
    /// assert!(next.kind.is_keyword(Keyword::If)); // 💣
    ///
    /// // Would not work since this would skip over the tree.
    ///
    /// let next = self.peek_raw(1).unwrap_or_else(SomeErr(..))?;
    /// assert!(next.kind.is_keyword(Keyword::If));
    /// ```
    pub fn peek_raw(&self, offset: usize) -> Option<&Token> {
        self.stream.get(self.pos.get() + offset)
    }

    /// Function to check if the token stream has been exhausted based on the
    /// current offset in the generator.
    #[inline]
    pub fn has_token(&self) -> bool {
        let length = self.stream.len();

        match length {
            0 => false,
            _ => self.pos.get() < self.stream.len(),
        }
    }

    /// Check if the [TokenStream] is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.stream.is_empty()
    }

    /// Get the total length of the [TokenStream].
    pub fn len(&self) -> usize {
        self.stream.len()
    }

    /// Get the [ByteRange] of the [TokenStream].
    pub fn range(&self) -> ByteRange {
        self.span
    }
}
