//! Definitions of the template dialects that bracketlint understands.

use std::path::Path;

use bl_ast::{BinOp, UnaryOp};
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

/// The template language that a [crate::Member] is written in. It is either
/// given with `--dialect`, or picked from the file extension, see
/// [Dialect::from_path].
///
/// The lowercase name of a dialect, e.g. `liquid`, is what `--dialect` accepts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Display, EnumString, IntoStaticStr, VariantArray,
)]
#[strum(serialize_all = "lowercase")]
pub enum Dialect {
    /// Django templates, used when nothing else identifies the dialect.
    #[default]
    Django,
    Jinja,
    Liquid,
    Twig,
}

impl Dialect {
    /// All of the dialects.
    pub const ALL: &'static [Dialect] = <Dialect as VariantArray>::VARIANTS;

    /// The name of the dialect, as accepted by `--dialect`.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// The file extension that identifies the dialect, if it has its own.
    /// Django templates use `.html`, which every dialect uses.
    pub const fn extension(self) -> Option<&'static str> {
        match self {
            Dialect::Django => None,
            Dialect::Jinja => Some("jinja"),
            Dialect::Liquid => Some("liquid"),
            Dialect::Twig => Some("twig"),
        }
    }

    /// The dialect that the extension of `path` identifies, if any.
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::ALL.iter().copied().find(|dialect| dialect.extension() == Some(extension))
    }

    /// Whether `{# ... #}` is a comment. Liquid has no such comment, so it is
    /// text there.
    pub fn has_hash_comments(self) -> bool {
        matches!(self, Dialect::Django | Dialect::Jinja | Dialect::Twig)
    }

    /// Whether `{% raw %}` blocks exist, whose contents are output as they are
    /// written. Django and Twig have `{% verbatim %}` instead.
    pub fn has_raw_blocks(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Liquid)
    }

    /// The markers that control the whitespace around a tag when they are
    /// written next to its delimiters, e.g. the `-` in `{%- if x -%}`. Django
    /// has none.
    pub fn trim_markers(self) -> &'static [char] {
        match self {
            Dialect::Django => &[],
            Dialect::Jinja => &['-', '+'],
            Dialect::Liquid => &['-'],
            Dialect::Twig => &['-', '~'],
        }
    }

    /// Whether a filter takes a list of arguments, which can be positional or
    /// keyword, e.g. `f: a, b, key: c`. The other dialects take a single
    /// argument, e.g. `f:a`.
    pub fn filter_supports_arg_list(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a tag's arguments are separated by commas, e.g.
    /// `{% cycle "a", "b" %}` or `{% include "card", product: product %}`.
    pub fn requires_comma_separated_args(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a tag's named arguments are written `name: value`, rather than
    /// `name=value`.
    pub fn named_args_use_colon(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// How the dialect spells `elif`, which is `elsif` in Liquid.
    pub fn elif_tag(self) -> &'static str {
        if self.is_liquid() { "elsif" } else { "elif" }
    }

    /// The tag that starts the body of a loop for when there is nothing to
    /// loop over, which is `empty` in Django and `else` in the other dialects.
    pub fn empty_loop_tag(self) -> &'static str {
        if self.is_django() { "empty" } else { "else" }
    }

    /// How tightly the binary operator `op` binds, as the binding powers of
    /// its left and right side. An operator with a higher binding power groups
    /// first, so `a or b and c` is `a or (b and c)`. `contains` is only a
    /// keyword in Liquid, so the other dialects never parse it, and only list
    /// it with their comparisons to cover every operator.
    pub fn infix_binding_power(self, op: BinOp) -> (u8, u8) {
        match self {
            // Django's own precedences, from `smartif.py`. `in` and `not in`
            // bind looser than the other comparisons, so `a in b == c` is
            // `a in (b == c)`.
            Dialect::Django => match op {
                BinOp::Or => left_assoc(6),
                BinOp::And => left_assoc(7),
                BinOp::In | BinOp::NotIn => left_assoc(9),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::Is
                | BinOp::IsNot
                | BinOp::Contains => left_assoc(10),
            },

            // Jinja's parser has a function for each level, from `parse_or`
            // to `parse_compare`, rather than numbers, so these go up in tens.
            // Jinja chains comparisons like Python, so `a == b > c` means
            // `a == b and b > c`, which is `(a == b) > c` here. A test binds
            // tighter than any operator.
            Dialect::Jinja => match op {
                BinOp::Or => left_assoc(10),
                BinOp::And => left_assoc(20),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::In
                | BinOp::NotIn
                | BinOp::Contains => left_assoc(40),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
            },

            // Liquid has no precedence between `and` and `or`, and evaluates
            // them from right to left, so `a and b or c` is `a and (b or c)`.
            Dialect::Liquid => match op {
                BinOp::And | BinOp::Or => right_assoc(2),
                BinOp::In | BinOp::NotIn => left_assoc(8),
                BinOp::Eq | BinOp::NotEq | BinOp::Is | BinOp::IsNot => right_assoc(8),
                BinOp::Gt | BinOp::GtEq | BinOp::Lt | BinOp::LtEq | BinOp::Contains => {
                    left_assoc(9)
                }
            },

            // Twig's own precedences, from `CoreExtension`. A test binds
            // tighter than the comparisons and `not`.
            Dialect::Twig => match op {
                BinOp::Or => left_assoc(10),
                BinOp::And => left_assoc(15),
                BinOp::Eq
                | BinOp::NotEq
                | BinOp::Lt
                | BinOp::LtEq
                | BinOp::Gt
                | BinOp::GtEq
                | BinOp::In
                | BinOp::NotIn
                | BinOp::Contains => left_assoc(20),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
            },
        }
    }

    /// How tightly the prefix operator `op` binds: its operand takes every
    /// binary operator whose left binding power is at least this, see
    /// [Dialect::infix_binding_power]. `-` binds tighter than any binary
    /// operator. `not` binds tighter than `and` and `or`, and in Twig also
    /// tighter than the comparisons, so `not a == b` is `(not a) == b` there
    /// and `not (a == b)` in the other dialects.
    pub fn prefix_binding_power(self, op: UnaryOp) -> u8 {
        match op {
            UnaryOp::Neg => u8::MAX,
            UnaryOp::Not => match self {
                Dialect::Django => 8,
                Dialect::Jinja => 30,
                Dialect::Liquid => 6,
                Dialect::Twig => 50,
            },
        }
    }

    /// Check if its currently Django dialect.
    #[inline]
    pub fn is_django(self) -> bool {
        self == Dialect::Django
    }

    /// Check if its currently Liquid dialect.
    #[inline]
    pub fn is_liquid(self) -> bool {
        self == Dialect::Liquid
    }
}

/// The binding powers of a left-associative operator that binds with `power`:
/// its right side only takes operators that bind tighter, so `a == b == c` is
/// `(a == b) == c`.
const fn left_assoc(power: u8) -> (u8, u8) {
    (power, power + 1)
}

/// The binding powers of a right-associative operator that binds with `power`:
/// its right side also takes operators that bind as tightly, so `a or b or c`
/// is `a or (b or c)`.
const fn right_assoc(power: u8) -> (u8, u8) {
    (power, power)
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_from_path() {
        assert_eq!(Dialect::from_path(Path::new("product.liquid")), Some(Dialect::Liquid));
        assert_eq!(Dialect::from_path(Path::new("base.twig")), Some(Dialect::Twig));
        assert_eq!(Dialect::from_path(Path::new("templates/page.jinja")), Some(Dialect::Jinja));
        assert_eq!(Dialect::from_path(Path::new("index.html")), None);
        assert_eq!(Dialect::from_path(Path::new("Makefile")), None);
    }

    #[test]
    fn test_name_round_trip() {
        for &dialect in Dialect::ALL {
            assert_eq!(dialect.name().parse::<Dialect>(), Ok(dialect));
            assert_eq!(dialect.to_string(), dialect.name());
        }

        assert_eq!(Dialect::Liquid.name(), "liquid");
        assert!("nunjucks".parse::<Dialect>().is_err());
    }
}
