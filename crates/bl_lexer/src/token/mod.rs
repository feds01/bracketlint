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
    /// Get the left hand side of the delimiter.
    pub const fn left(&self) -> &'static str {
        match self {
            Delimiter::Paren => "(",
            Delimiter::Percent => "{%",
            Delimiter::Brace => "{{",
            Delimiter::Bracket => "[",
        }
    }

    /// Get the right hand side of the delimiter.
    pub const fn right(&self) -> &'static str {
        match self {
            Delimiter::Paren => ")",
            Delimiter::Percent => "%}",
            Delimiter::Brace => "}}",
            Delimiter::Bracket => "]",
        }
    }

    /// Get the width of the delimiter.
    pub const fn width(&self) -> usize {
        match self {
            // `(`, `)`, `{`, `}`, `[`, `]`
            Delimiter::Paren | Delimiter::Bracket => 1,
            // `{%`, `%}`, `{{`, `}}`
            Delimiter::Percent | Delimiter::Brace => 2,
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

/// Flags for the number token.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum NumberFlags {
    /// A floating point number.
    Float,
    /// An integer number.
    Int,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// Colon, `:`
    Colon,

    /// Comma, `,`
    Comma,

    /// Dot, `.`
    Dot,

    /// Assignment, `=`
    Eq,

    /// Equal to, `==`
    EqEq,

    /// Not equal to, `!=`
    NotEq,

    /// Greater than, `>`
    Gt,

    /// Greater than or equal to, `>=`
    GtEq,

    /// Less than, `<`
    Lt,

    /// Less than or equal to, `<=`
    LtEq,

    /// Minus, `-`
    Minus,

    /// Plus, `+`
    Plus,

    /// Exclamation, `!`
    Exclamation,

    /// Pound, `#`
    Pound,

    /// Pipe, `|`
    Pipe,

    /// Percent, `%`
    Percent,

    /// An identifier.
    Ident,

    /// A number literal.
    Number(NumberFlags),

    /// A string literal.
    ///
    /// This is a token that represents a string literal, e.g. `"hello world"`.
    ///
    /// N.B. String literals don't support escaping, everything within the
    /// string is considered verbatim.
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

impl TokenKind {
    /// Check if a token is a literal value.
    pub fn is_lit(&self) -> bool {
        matches!(
            self,
            TokenKind::Str
                | TokenKind::Number(_)
                | TokenKind::Keyword(Keyword::True | Keyword::False)
        )
    }

    /// Check if a token is a tree token.
    pub fn is_tree(&self) -> bool {
        matches!(self, TokenKind::Tree(_, _))
    }

    /// Check if a token is a tree token.
    pub fn is_percent_tree(&self) -> bool {
        matches!(self, TokenKind::Tree(Delimiter::Percent, _))
    }

    pub fn is_soft_keyword(&self) -> bool {
        matches!(self, TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::Load))
    }


    pub fn is_unary_op(&self) -> bool {
        matches!(self, TokenKind::Keyword(Keyword::Not) | TokenKind::Minus)
    }


    /// Check if a token starts an expression.
    pub fn starts_expr(&self) -> bool {
        match self {
            kind if kind.is_lit() => true,
            TokenKind::Ident => true,
            TokenKind::Keyword(keyword) if keyword.identifier_like() => true,
            TokenKind::LeftDelim(Delimiter::Paren | Delimiter::Bracket | Delimiter::Brace) => true,
            _ => false,
        }
    }

    pub fn is_ident_like(&self) -> bool {
        match self {
            TokenKind::Ident => true,
            TokenKind::Keyword(kwd) => kwd.identifier_like(),
            _ => false,
        }
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Eq => write!(f, "="),
            TokenKind::EqEq => write!(f, "=="),
            TokenKind::NotEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Exclamation => write!(f, "!"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Pound => write!(f, "#"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::LeftDelim(delim) => write!(f, "{}", delim.left()),
            TokenKind::RightDelim(delim) => write!(f, "{}", delim.right()),
            TokenKind::Tree(delim, _) => write!(f, "{}...{}", delim.left(), delim.right()),
            TokenKind::Str => write!(f, "<string>"),
            TokenKind::Keyword(kwd) => kwd.fmt(f),
            TokenKind::Ident => write!(f, "<identifier>"),
            TokenKind::Number(_) => write!(f, "<number>"),
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
