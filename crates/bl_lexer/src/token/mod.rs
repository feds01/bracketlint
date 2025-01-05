//! The lexer token package, contains all of the utilities for working with
//! tokens, including the definitions of the tokens themselves.

pub mod cursor;
pub mod keywords;

use core::fmt;

use bl_ast::ByteRange;
use derive_more::Constructor;
pub use keywords::Keyword;

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

impl Delimiter {
    pub fn left(&self) -> &'static str {
        match self {
            Delimiter::Paren => "(",
            Delimiter::Percent => "{%",
            Delimiter::Brace => "{{",
            Delimiter::Bracket => "[",
        }
    }

    pub fn right(&self) -> &'static str {
        match self {
            Delimiter::Paren => ")",
            Delimiter::Percent => "%}",
            Delimiter::Brace => "}}",
            Delimiter::Bracket => "]",
        }
    }
}

impl TryFrom<char> for Delimiter {
    type Error = ();

    fn try_from(c: char) -> Result<Delimiter, Self::Error> {
        match c {
            '(' | ')' => Ok(Delimiter::Paren),
            '{' | '}' => Ok(Delimiter::Brace),
            '[' | ']' => Ok(Delimiter::Bracket),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Colon,
    Comma,
    Dot,
    Eq,
    EqEq,
    Gt,
    GtEq,
    Lt,
    LtEq,
    Minus,
    Exclamation,
    Pound,
    Percent,
    Ident,
    Number,
    Str,

    /// Effectively a hunk of text within the source that isn't tokenised from
    /// the context of the template.
    Text,

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

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Eq => write!(f, "="),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Exclamation => write!(f, "!"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Pound => write!(f, "#"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::LeftDelim(delim) => write!(f, "{}", delim.left()),
            TokenKind::RightDelim(delim) => write!(f, "{}", delim.right()),
            TokenKind::Tree(delim, _) => write!(f, "{}...{}", delim.left(), delim.right()),
            TokenKind::Str => write!(f, "<string>"),
            TokenKind::Keyword(kwd) => kwd.fmt(f),
            TokenKind::Ident => write!(f, "<identifier>"),
            TokenKind::Number => write!(f, "<number>"),
            TokenKind::Text => write!(f, "<text>"),
            TokenKind::Comment => write!(f, "<comment>"),

            TokenKind::Unexpected(atom) => write!(f, "{atom}"),
            TokenKind::Err => write!(f, "<error>"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Constructor)]
pub struct Token {
    /// The kind of token.
    pub kind: TokenKind,

    /// The [ByteRange] of the token within the source.
    pub span: ByteRange,
}
