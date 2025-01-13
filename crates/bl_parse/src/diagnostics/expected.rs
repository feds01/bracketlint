//! Defines a structure used to represent the tokens that the
//! parser expected at some point during parsing. [ExpectedItem]
//! is represented using [`bitflags!`] in order to allow for encoding
//! multiple [ExpectedItem]s into a single [ExpectedItem] value.

use std::fmt;

use bitflags::bitflags;
use bl_lexer::token::{Delimiter, TokenKind};
use bl_reporting::SequenceDisplay;

bitflags! {
    /// Defines what expected items could be encountered in a given context.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ExpectedItem: u32 {
        /// An identifier.
        const Ident = 1 << 1;

        /// A literal token.
        const Literal = 1 << 2;

        /// A minus token.
        const Minus = 1 << 3;

        /// A plus token
        const Plus = 1 << 4;

        /// A dot.
        const Dot = 1 << 6;

        /// An exclamation mark token.
        const Exclamation = 1 << 7;

        /// A comma token.
        const Comma = 1 << 10;

        /// A colon token.
        const Colon = 1 << 11;

        /// An equal sign token.
        const Eq = 1 << 12;

        /// A comparison operator, `==`.
        const EqEq = 1 << 13;

        /// A `<` delimiter.
        const Lt = 1 << 14;

        /// A `<=` delimiter.
        const LtEq = 1 << 15;

        /// A `>` delimiter
        const Gt = 1 << 16;

        /// A `>=` delimiter
        const GtEq = 1 << 17;

        /// Left parenthesis
        const LeftParen = 1 << 18;

        /// Right parenthesis
        const RightParen = 1 << 19;

        /// Left brace
        const LeftBrace = 1 << 20;

        /// Right brace
        const RightBrace = 1 << 21;

        /// Left bracket
        const LeftBracket = 1 << 22;

        /// Right bracket
        const RightBracket = 1 << 23;

        /// Convenient grouping of `operator`.
        const Op = Self::Minus.bits()
                 | Self::Lt.bits()
                 | Self::Gt.bits()
                | Self::EqEq.bits()
                | Self::LtEq.bits()
                | Self::GtEq.bits();


        /// Convenient left-wise delimiter mask.
        const DelimLeft = Self::LeftParen.bits()
                        | Self::LeftBrace.bits()
                        | Self::LeftBracket.bits();

        /// Convenient definition for the beginning of an expression.
        const Expr = Self::Ident.bits()
                   | Self::Literal.bits()
                   | Self::LeftParen.bits()
                   | Self::LeftBracket.bits()
                   | Self::LeftBrace.bits();
    }
}

impl fmt::Display for ExpectedItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut toks = vec![];

        for kind in self.iter() {
            match kind {
                ExpectedItem::Ident => toks.push("identifier"),
                ExpectedItem::Literal => toks.push("literal"),
                ExpectedItem::Comma => toks.push(","),
                ExpectedItem::Colon => toks.push(":"),
                ExpectedItem::Minus => toks.push("-"),
                ExpectedItem::Dot => toks.push("."),
                ExpectedItem::Eq => toks.push("="),
                ExpectedItem::EqEq => toks.push("=="),
                ExpectedItem::Lt => toks.push("<"),
                ExpectedItem::LtEq => toks.push("<="),
                ExpectedItem::Gt => toks.push(">"),
                ExpectedItem::GtEq => toks.push(">="),
                ExpectedItem::LeftParen => toks.push("("),
                ExpectedItem::RightParen => toks.push(")"),
                ExpectedItem::LeftBrace => toks.push("{"),
                ExpectedItem::RightBrace => toks.push("}"),
                ExpectedItem::LeftBracket => toks.push("["),
                ExpectedItem::RightBracket => toks.push("]"),
                _ => unreachable!(),
            }
        }

        write!(f, "{}", SequenceDisplay::either(&toks))
    }
}

impl From<TokenKind> for ExpectedItem {
    fn from(value: TokenKind) -> Self {
        match value {
            TokenKind::Eq => ExpectedItem::Eq,
            TokenKind::Lt => ExpectedItem::Lt,
            TokenKind::LtEq => ExpectedItem::LtEq,
            TokenKind::Gt => ExpectedItem::Gt,
            TokenKind::GtEq => ExpectedItem::GtEq,
            TokenKind::EqEq => ExpectedItem::EqEq,
            TokenKind::Minus => ExpectedItem::Minus,
            TokenKind::Dot => ExpectedItem::Dot,
            TokenKind::Colon => ExpectedItem::Colon,
            TokenKind::Comma => ExpectedItem::Comma,
            TokenKind::Ident => ExpectedItem::Ident,
            TokenKind::Keyword(_) => ExpectedItem::Ident,
            token if token.is_lit() => ExpectedItem::Literal,
            _ => unreachable!("unexpected token kind when deriving expected item: {:?}", value),
        }
    }
}

impl From<Delimiter> for ExpectedItem {
    fn from(value: Delimiter) -> Self {
        match value {
            Delimiter::Paren => ExpectedItem::LeftParen,
            Delimiter::Bracket => ExpectedItem::RightBracket,
            Delimiter::Brace => ExpectedItem::RightBracket,
            Delimiter::Percent => ExpectedItem::Gt,
        }
    }
}
