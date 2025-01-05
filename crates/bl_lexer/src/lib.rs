//! BL lexer implementation for the parser.
#![feature(cell_update)]

pub mod diagnostics;
pub mod token;

use std::cell::Cell;

use bl_ast::{ByteRange, SourceId, Span, SpannedSource};
use bl_reporting::DiagnosticsMut;
use diagnostics::{LexerDiagnostics, LexerError, LexerErrorKind};
use token::{Delimiter, Keyword, Token, TokenKind};

/// Representing the end of stream, or the initial character that is set as
/// 'prev' in a [Lexer] since there is no character before the start.
const EOF_CHAR: char = '\0';
/// Metadata that the lexer produces once it finished processing the given
/// input.
pub struct LexerMetadata {
    /// Token tree store, essentially a collection of token trees that are
    /// produced when the lexer encounters bracketed token streams.
    pub tokens: Vec<Token>,

    /// Diagnostics produced by the lexer.
    pub diagnostics: LexerDiagnostics,
}

/// The lexer itself, which is responsible for converting a stream of characters
/// into a stream of tokens.
///
/// It is intended that a lexer is "consumed" after it has been used to produce
/// a stream of tokens. If the lexer encounters a fatal error, this is still
/// considered to be a successful operation, and the lexer will still produce a
/// stream of tokens that it was able to produce before the error occurred.
pub struct Lexer<'lex> {
    /// The source that the lexer is processing.
    pub source: SpannedSource<'lex>,

    /// Location of the lexer in the current stream.
    offset: Cell<usize>,

    /// The ID of the member that the lexer is processing, useful for error
    /// reporting.
    pub id: SourceId,

    /// Diagnostics that the lexer has produced.
    pub diagnostics: LexerDiagnostics,

    pub has_fatal_error: bool,

    /// The tokens that the lexer has produced.
    pub tokens: Vec<Token>,
}

impl<'lex> Lexer<'lex> {
    pub fn new(source: SpannedSource<'lex>, id: SourceId) -> Self {
        Self {
            source,
            id,
            diagnostics: LexerDiagnostics::default(),
            tokens: Vec::new(),
            has_fatal_error: false,
            offset: Cell::new(0),
        }
    }

    /// Emit an error into [LexerDiagnostics] and also
    /// set `has_fatal_error` flag to true so that the
    /// lexer terminates on the next advancement.
    #[inline]
    fn emit_fatal_error(&mut self, kind: LexerErrorKind, span: ByteRange) -> TokenKind {
        self.has_fatal_error = true;
        self.emit_error(kind, span)
    }

    /// Put an error into the [LexerDiagnostics], whilst returning a
    /// [TokenKind::Err] in place of a lexed token.
    #[inline]
    fn emit_error(&mut self, kind: LexerErrorKind, span: ByteRange) -> TokenKind {
        self.diagnostics.add_error(LexerError { kind, span: Span { range: span, id: self.id } });

        TokenKind::Err
    }

    /// Moves to the next character.
    #[inline]
    fn next(&mut self) -> Option<char> {
        let slice = unsafe { self.as_slice() };
        let ch = slice.chars().next()?;
        self.offset.update(|x| x + ch.len_utf8());
        Some(ch)
    }

    /// Returns nth character relative to the current position.
    /// If requested position doesn't exist, `EOF_CHAR` is returned.
    /// However, getting `EOF_CHAR` doesn't always mean actual end of file,
    /// it should be checked with `is_eof` method.
    fn nth_char(&self, n: usize) -> char {
        let slice = unsafe { self.as_slice() };

        slice.chars().nth(n).unwrap_or(EOF_CHAR)
    }

    /// Peeks the next symbol from the input stream without consuming it.
    #[inline]
    fn peek(&self) -> char {
        self.nth_char(0)
    }

    /// Peeks the second symbol from the input stream.
    #[inline]
    fn peek_second(&self) -> char {
        self.nth_char(1)
    }

    #[inline]
    fn skip_ascii(&self) {
        self.offset.update(|x| x + 1);
    }

    /// Get the remaining un-lexed contents as a raw string.
    #[inline]
    unsafe fn as_slice(&self) -> &str {
        let offset = self.offset.get();

        // ##Safety: We rely that the byte offset is correctly computed when stepping
        // over the characters in the iterator.
        unsafe { std::str::from_utf8_unchecked(self.source.0.as_bytes().get_unchecked(offset..)) }
    }

    /// Checks if there is nothing more to consume.
    fn is_eof(&self) -> bool {
        self.source.0.len() == self.offset.get()
    }

    /// Returns amount of already consumed symbols.
    #[inline(always)]
    fn len_consumed(&self) -> usize {
        self.offset.get() - 1
    }

    /// Eat while the condition holds, and discard any characters that it
    /// encounters whilst eating the input, this is useful because in some
    /// cases we don't want to preserve what the token represents, such as
    /// comments or white-spaces...
    fn eat_while_and_discard(&self, mut condition: impl FnMut(char) -> bool) {
        if self.is_eof() {
            return;
        }

        let slice = unsafe { self.as_slice() };
        let index = slice.find(|c| !condition(c)).unwrap_or(slice.len());
        self.offset.update(|x| x + index);
    }

    pub fn advance_token(&mut self) -> Option<Token> {
        let offset = self.offset.get();

        self.eat_while_and_discard(char::is_whitespace);
        // If we reach here, that means we can return a token.
        let location = ByteRange::new(offset, self.len_consumed());
        Some(Token::new(kind, location))
    }

    /// Consume the lexer and produce a stream of tokens.
    pub fn tokenise(mut self) -> LexerMetadata {
        while let Some(token) = self.advance_token() {
            self.tokens.push(token);
        }

        LexerMetadata { tokens: self.tokens, diagnostics: self.diagnostics }
    }

    fn string(&mut self, start: char) -> TokenKind {
        let is_double = start == '"';
        let mut closed = false;

        let start = self.offset.get();

        while let Some(c) = self.next() {
            match c {
                '"' if is_double => {
                    closed = true;
                    break;
                }
                '\'' if !is_double => {
                    closed = true;
                    break;
                }
                // '\\' => match self.escaped_char(false) {
                //     Ok(ch) => continue,
                //     Err(err) => {
                //         self.add_error(err);
                //         return TokenKind::Err;
                //     }
                // },
                _c => continue,
            }
        }

        // Report that the literal is unclosed and set the error as being fatal
        if !closed {
            return self.emit_fatal_error(
                LexerErrorKind::UnclosedStringLit,
                ByteRange::new(start, self.len_consumed()),
            );
        }

        // Avoid interning on a global level until later, we check locally if we've
        // seen the string, and then push it into our literal map if we haven't...
        TokenKind::Str
    }
}
