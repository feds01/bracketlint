//! Macros that expand to patterns over [Token]s, so that a run of tokens can be
//! matched without writing out each `Token { kind: TokenKind::…, .. }`.
//!
//! [Token]: crate::token::Token

/// Expands to a pattern that matches a [Token](crate::token::Token) of the
/// given [TokenKind](crate::token::TokenKind) variant, e.g. `tok!(Eq)` matches
/// a `=`. The variant must not hold any data, use `kw!` for keywords.
#[macro_export]
macro_rules! tok {
    ($kind:ident) => {
        $crate::token::Token { kind: $crate::token::TokenKind::$kind, .. }
    };
}

/// Expands to a pattern that matches a [Token](crate::token::Token) of the
/// given [Keyword](crate::token::Keyword), e.g. `kw!(Raw)` matches `raw`.
#[macro_export]
macro_rules! kw {
    ($keyword:ident) => {
        $crate::token::Token {
            kind: $crate::token::TokenKind::Keyword($crate::token::Keyword::$keyword),
            ..
        }
    };
}

/// Expands to a slice pattern that matches a whole token tree: the tree token
/// with the given [Delimiter](crate::token::Delimiter), followed by a pattern
/// for each token inside it, e.g. `tree!(Percent, [kw!(Raw)])` matches the
/// tokens of `{% raw %}`.
///
/// The tree token stores how many tokens are inside the tree, which a pattern
/// can only check against a literal, so there is an arm for each number of
/// inner patterns. Each inner pattern has to match exactly one token, so it
/// can't be a nested tree or `..`.
#[macro_export]
macro_rules! tree {
    ($delimiter:ident, [$a:pat]) => {
        [$crate::tree!(@open $delimiter, 1), $a]
    };
    ($delimiter:ident, [$a:pat, $b:pat]) => {
        [$crate::tree!(@open $delimiter, 2), $a, $b]
    };
    ($delimiter:ident, [$a:pat, $b:pat, $c:pat]) => {
        [$crate::tree!(@open $delimiter, 3), $a, $b, $c]
    };
    ($delimiter:ident, [$a:pat, $b:pat, $c:pat, $d:pat]) => {
        [$crate::tree!(@open $delimiter, 4), $a, $b, $c, $d]
    };
    ($delimiter:ident, [$($inner:pat),*]) => {
        compile_error!("`tree!` takes 1 to 4 inner patterns, add an arm for more")
    };
    (@open $delimiter:ident, $len:literal) => {
        $crate::token::Token {
            kind: $crate::token::TokenKind::Tree($crate::token::Delimiter::$delimiter, $len),
            ..
        }
    };
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bl_ast::{LineRanges, SourceId, SpannedSource};
    use bl_workspace::Dialect;

    use crate::{Lexer, token::Token};

    /// Lex `source` as a Jinja template.
    fn lex(source: &str) -> Vec<Token> {
        let path = PathBuf::new();
        let line_ranges = LineRanges::new_from_str(source);
        let spanned = SpannedSource::new(source, &path, &line_ranges);

        Lexer::new(spanned, SourceId::default(), Dialect::Jinja).tokenise().tokens
    }

    #[test]
    fn tree_matches_the_tokens_of_a_tag() {
        assert!(matches!(lex("{% raw %}")[..], tree!(Percent, [kw!(Raw)])));
        assert!(matches!(lex("{% load x %}")[..], tree!(Percent, [kw!(Load), tok!(Ident)])));
        assert!(matches!(
            lex("{{ x.y }}")[..],
            tree!(Brace, [tok!(Ident), tok!(Dot), tok!(Ident)])
        ));
        assert!(matches!(
            lex("{% for x in y %}")[..],
            tree!(Percent, [kw!(For), tok!(Ident), kw!(In), tok!(Ident)])
        ));
    }

    #[test]
    fn tree_does_not_match_the_start_of_a_longer_tag() {
        // The first two tokens are a `{% %}` tree and `raw`, but the tree has
        // two tokens inside it rather than one.
        assert!(!matches!(lex("{% raw x %}")[..2], tree!(Percent, [kw!(Raw)])));
    }
}
