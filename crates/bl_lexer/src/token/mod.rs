//! The lexer token package, contains all of the utilities for working with
//! tokens, including the definitions of the tokens themselves.

pub mod cursor;

use bl_ast::{ByteRange, Identifier};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Keyword {
    For,
    In,
    If,
    Elif,
    Else,
    Block,
    Endblock,
    As,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Delimiter {
    /// Parenthesis, `(` or `)`
    Paren,

    /// Percent braces `{%` or `%}`
    Percent,

    /// Braces, `{{` or `}}`
    Brace,

    /// Bracket, `[` or `]`
    Bracket,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Colon,
    Comma,
    Dot,
    Eq,
    Gt,
    Lt,
    Minus,
    Plus,
    Exclamation,
    Pound,
    Ident(Identifier),
    Number,
    Str,

    /// A comment token, a hunk of text that should be structurally
    /// ignored, but preserved when rendering the template.
    Comment,

    /// Tree a token indicating the start of a token tree, i.e. some
    /// delimited block of tokens. The `u32` is the length of the tokens
    /// within the tree excluding the delimiters.
    Tree(Delimiter, u32),

    /// Keyword
    Keyword(Keyword),

    /// Delimiters `(`, `{`, `[`, doesn't include `<`.
    LeftDelim(Delimiter),

    /// Delimiters `)`, `}`, `]`, doesn't include `>`.
    RightDelim(Delimiter),

    /// A token that was unexpected by the lexer, e.g. a unicode symbol not
    /// within string literal.
    Unexpected(char),

    /// Error token within the tokenisation process, essentially aiding deferred
    /// error reporting
    Err,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,

    /// The [ByteRange] of the token within the source.
    pub span: ByteRange,
}
