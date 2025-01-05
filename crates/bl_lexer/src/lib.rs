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

/// Information about a tree that is being lexed by the [Lexer]. Includes
/// information about the start of the lexer (in the token buffer), and if the
/// lexer consumed a delimiter token.
#[derive(Clone, Copy, Debug)]
struct TreeInfo {
    /// An index into the stream of tokens, pointing to where the tree begins.
    start: usize,

    /// The kind of [Delimiter] that the tree is.
    delimiter: Option<Delimiter>,
}

enum ShouldSkip {
    Yes,
    No,
}

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

    tree: Cell<Option<TreeInfo>>,

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
            tree: Cell::new(None),
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

    /// Eat while the condition holds, and produces a slice from where it began
    /// to eat the input and where it finished, this is sometimes beneficial
    /// as the slice doesn't have to be re-allocated as a string.
    fn eat_while_and_slice(&self, condition: impl FnMut(char) -> bool) -> &str {
        if self.is_eof() {
            return "";
        }

        // Capture the range of the slice, and then finally update the offset
        let start = self.offset.get();
        self.eat_while_and_discard(condition);
        let consumed = self.offset.get();
        let end = if consumed == start { start } else { consumed - 1 };

        self.source.hunk(ByteRange::new(start, end))
    }

    pub fn advance_token(&mut self) -> Option<Token> {
        let offset = self.offset.get();

        self.eat_while_and_discard(char::is_whitespace);

        let on_tree = |this: &mut Self, delimiter: Delimiter| {
            this.tokens.push(Token::new(
                TokenKind::Tree(delimiter, 0),
                ByteRange::new(offset, this.len_consumed()),
            ));
            this.eat_token_tree(delimiter);

            if this.has_fatal_error {
                return None;
            }

            // Immediately try to index the next token...
            this.advance_token()
        };

        let kind = if let Some(mut info) = self.tree.get() {
            match self.next()? {
                '%' => match self.peek() {
                    '}' => {
                        info.delimiter = Some(Delimiter::Percent);
                        self.tree.set(Some(info));

                        self.skip_ascii();
                        return None;
                    }
                    _ => TokenKind::Percent,
                },
                '}' => match self.peek() {
                    '}' => {
                        info.delimiter = Some(Delimiter::Brace);
                        self.tree.set(Some(info));
                        self.skip_ascii();
                        return None;
                    }
                    _ => TokenKind::RightDelim(Delimiter::Brace),
                },
                c @ (')' | ']') => {
                    info.delimiter = Some(Delimiter::try_from(c).unwrap());
                    self.tree.set(Some(info));
                    self.skip_ascii();
                    return None;
                }
                ch @ ('(' | '[') => {
                    return on_tree(self, Delimiter::try_from(ch).unwrap());
                }
                c if is_ident_start(c) => self.ident(c),
                '0'..='9' => self.number(ShouldSkip::No),
                ':' => TokenKind::Colon,
                ',' => TokenKind::Comma,
                '.' => TokenKind::Dot,
                '=' => match self.peek() {
                    '=' => {
                        self.skip_ascii();
                        TokenKind::EqEq
                    }
                    _ => TokenKind::Eq,
                },
                '>' => match self.peek() {
                    '=' => {
                        self.skip_ascii();
                        TokenKind::GtEq
                    }
                    _ => TokenKind::Gt,
                },
                '<' => match self.peek() {
                    '=' => {
                        self.skip_ascii();
                        TokenKind::LtEq
                    }
                    _ => TokenKind::Lt,
                },
                '-' => match self.peek() {
                    c if c.is_ascii_digit() => self.number(ShouldSkip::Yes),
                    _ => TokenKind::Minus,
                },
                c @ ('\'' | '"') => self.string(c),
                c => TokenKind::Unexpected(c),
            }
        } else {
            match self.next()? {
                '{' => {
                    // we need to handle whether this a variable block, we have a few options:
                    //
                    // 1. We have a variable block, which is `{{ ... }}`
                    // 2. We have a function block, which is `{% ... %}`
                    // 3. We have a comment block, which is `{# ... #}`
                    match self.peek() {
                        c @ ('%' | '{') => {
                            self.skip_ascii();

                            let delimiter =
                                if c == '%' { Delimiter::Percent } else { Delimiter::Brace };
                            return on_tree(self, delimiter);
                        }
                        '#' => {
                            self.skip_ascii();
                            self.comment()
                        }
                        _ => self.text(),
                    }
                }
                // We assume that this is just "text"
                _ => self.text(),
            }
        };

        // If we reach here, that means we can return a token.
        let location = ByteRange::new(offset, self.len_consumed());
        Some(Token::new(kind, location))
    }

    /// This will essentially recursively consume tokens until it reaches the
    /// right hand-side variant of the provided delimiter. If no delimiter
    /// is reached, but the stream has reached EOF, this is reported
    /// as an error because it is essentially an un-closed block. This kind of
    /// behaviour is desired and avoids performing complex delimiter depth
    /// analysis later on.
    fn eat_token_tree(&mut self, delimiter: Delimiter) -> TokenKind {
        let delim_offset = self.offset.get() - 2; // we need to ge the previous location to accurately denote the error...

        // we need to reset self.prev here as it might be polluted with previous token
        // trees
        let tree = Cell::new(Some(TreeInfo { start: self.tokens.len(), delimiter: None }));
        self.tree.swap(&tree);

        // `None` here doesn't just mean EOF, it could also be that
        // the next token failed to be parsed.
        while !self.is_eof() {
            match self.advance_token() {
                Some(token) => self.tokens.push(token),
                None => break,
            }
        }

        // ##Note: Now that we have done parsing the inner tree (with or without error),
        // we want to put the `old` tree value back into `self.tree`, and
        // retrieve the one that we were working with. Beyond this point,
        // everyone should refer to `tree`, not `self.tree` since this is now
        // the old one.
        self.tree.swap(&tree);

        // If there is a fatal error, then we need to abort
        if self.has_fatal_error {
            return TokenKind::Err;
        }

        match tree.get() {
            Some(TreeInfo { delimiter: Some(d), start, .. }) if d == delimiter => {
                // Update the tree token with the length of the tree.
                self.tokens[start - 1] = Token::new(
                    TokenKind::Tree(delimiter, (self.tokens.len() - start) as u32),
                    ByteRange::new(delim_offset, self.len_consumed()),
                );

                // ##Hack: This token won't be put into the token stream, it's just
                // a dummy token to denote the end of the tree.
                TokenKind::RightDelim(delimiter)
            }
            _ => {
                // backtrack a single token, so that if other trees exist, they can
                // still be properly handled.
                self.offset.set(self.offset.get() - 1);

                self.emit_error(
                    LexerErrorKind::Unclosed(delimiter),
                    ByteRange::singleton(delim_offset),
                )
            }
        }
    }

    /// Consume the lexer and produce a stream of tokens.
    pub fn tokenise(mut self) -> LexerMetadata {
        while let Some(token) = self.advance_token() {
            self.tokens.push(token);
        }

        LexerMetadata { tokens: self.tokens, diagnostics: self.diagnostics }
    }

    fn number(&mut self, skip: ShouldSkip) -> TokenKind {
        // record the start location of the literal
        let start = self.offset.get() - (skip as usize);

        // If we didn't get a radix, then we eat all the digits that we can, and then
        // check if it is a float literal.
        self.eat_while_and_slice(move |c| c.is_ascii_digit());

        match self.peek() {
            '.' if !is_ident_start(self.peek_second()) => {
                self.skip_ascii();
                self.eat_while_and_slice(move |c| c.is_ascii_digit());
                self.eat_float_lit(start)
            }
            // Immediate exponent
            'e' | 'E' => self.eat_float_lit(start),
            _ => TokenKind::Number,
        }
    }

    fn eat_float_lit(&mut self, start: usize) -> TokenKind {
        if !matches!(self.peek(), 'e' | 'E') {
            return TokenKind::Number;
        }

        self.skip_ascii(); // consume the exponent

        // Check if there is a sign before the digits start in the exponent...
        if self.peek() == '-' {
            self.skip_ascii();
        };

        // Check that there is at least on digit in the exponent
        if self.peek() == EOF_CHAR {
            return self.emit_error(
                LexerErrorKind::MissingExponentDigits,
                ByteRange::new(start, self.len_consumed()),
            );
        }

        if self.eat_decimal_digits(10).parse::<i32>().is_err() {
            self.emit_error(
                LexerErrorKind::InvalidFloatExponent,
                ByteRange::new(start, self.len_consumed()),
            )
        } else {
            TokenKind::Number
        }
    }

    /// Consume only decimal digits up to encountering a non-decimal digit.
    fn eat_decimal_digits(&self, radix: u32) -> &str {
        self.eat_while_and_slice(move |c| c.is_digit(radix))
    }

    fn text(&mut self) -> TokenKind {
        // keep eating until we find a delimiter
        while let Some(c) = self.next() {
            match c {
                '{' => match self.peek() {
                    '%' | '{' => {
                        self.offset.update(|x| x - 1);
                        break;
                    }
                    _ => continue,
                },
                _ => continue,
            }
        }

        TokenKind::Text
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

    fn ident(&mut self, first: char) -> TokenKind {
        debug_assert!(is_ident_start(first));

        let start = self.offset.get() - first.len_utf8();
        self.eat_while_and_discard(is_id_continue);
        let name = &self.source.0[start..self.offset.get()];

        if let Ok(keyword) = Keyword::try_from(name) {
            TokenKind::Keyword(keyword)
        } else {
            TokenKind::Ident
        }
    }

    fn comment(&mut self) -> TokenKind {
        while let Some(c) = self.next() {
            if c == '#' && self.peek() == '}' {
                self.skip_ascii();
                break;
            }
        }

        TokenKind::Comment
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// True if `c` is valid as a non-first character of an identifier.
pub(crate) fn is_id_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
