//! Macros that expand to patterns over [Token]s, so that a run of tokens can be
//! matched without writing out each `Token { kind: TokenKind::…, .. }`.
//!
//! [Token]: crate::token::Token

/// Expands to a pattern that matches a [Token](crate::token::Token) of the
/// given [TokenKind](crate::token::TokenKind) variant, e.g. `tok!(Eq)` matches
/// a `=`. A variant that holds data takes patterns for it, e.g.
/// `tok!(Number(_))`. Use `kw!` for keywords.
#[macro_export]
macro_rules! tok {
    ($kind:ident) => {
        $crate::token::Token { kind: $crate::token::TokenKind::$kind, .. }
    };
    ($kind:ident($($data:pat),+)) => {
        $crate::token::Token { kind: $crate::token::TokenKind::$kind($($data),+), .. }
    };
}

/// Expands to a pattern that matches a [Token](crate::token::Token) of the
/// given [Keyword](bl_ast::Keyword), e.g. `kw!(Raw)` matches `raw`.
#[macro_export]
macro_rules! kw {
    ($keyword:ident) => {
        $crate::token::Token {
            kind: $crate::token::TokenKind::Keyword(::bl_ast::Keyword::$keyword),
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
/// can only check against a literal, so the macro counts the inner patterns
/// with `${count(..)}`. Each inner pattern has to match exactly one token, so
/// it can't be a nested tree or `..`. A crate that uses this macro needs
/// `#![feature(macro_metavar_expr)]`.
#[macro_export]
macro_rules! tree {
    ($delimiter:ident, [$($inner:pat),* $(,)?]) => {
        [
            $crate::token::Token {
                kind: $crate::token::TokenKind::Tree(
                    $crate::token::Delimiter::$delimiter,
                    ${count($inner)},
                ),
                ..
            },
            $($inner),*
        ]
    };
}

#[cfg(test)]
mod tests {
    use bl_ast::Dialect;

    use crate::tests::lex;

    #[test]
    fn tree_matches_the_tokens_of_a_tag() {
        let jinja = |source: &str| lex(source, Dialect::Jinja);

        assert!(matches!(jinja("{% raw %}")[..], tree!(Percent, [kw!(Raw)])));
        assert!(matches!(jinja("{% load x %}")[..], tree!(Percent, [kw!(Load), tok!(Ident)])));
        assert!(matches!(
            jinja("{{ x.y }}")[..],
            tree!(Brace, [tok!(Ident), tok!(Dot), tok!(Ident)])
        ));
        assert!(matches!(
            jinja("{% for x in y %}")[..],
            tree!(Percent, [kw!(For), tok!(Ident), kw!(In), tok!(Ident)])
        ));
        assert!(matches!(
            jinja("{% for k, v in items %}")[..],
            tree!(Percent, [kw!(For), tok!(Ident), tok!(Comma), tok!(Ident), kw!(In), tok!(Ident)])
        ));
    }

    #[test]
    fn tree_does_not_match_the_start_of_a_longer_tag() {
        // The first two tokens are a `{% %}` tree and `raw`, but the tree has
        // two tokens inside it rather than one.
        assert!(!matches!(lex("{% raw x %}", Dialect::Jinja)[..2], tree!(Percent, [kw!(Raw)])));
    }

    #[test]
    fn tok_matches_the_data_of_a_variant() {
        let tokens = lex("{% (-1) %}", Dialect::Jinja);

        assert!(matches!(
            tokens[..],
            tree!(Percent, [tok!(Tree(crate::token::Delimiter::Paren, 1)), tok!(Number(_))])
        ));
        assert!(!matches!(tokens[..], tree!(Percent, [tok!(Tree(_, 2)), tok!(Number(_))])));
    }
}
