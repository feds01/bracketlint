//! Definitions of the template dialects that bracketlint understands.

use std::path::Path;

use serde::Deserialize;
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

use crate::{
    BinOp, UnaryOp,
    keywords::{KEYWORDS, Keyword, LIQUID_KEYWORDS},
};

/// The template language that a file is written in. It is either given with
/// `--dialect`, or picked from the file extension, see [Dialect::from_path].
///
/// The lowercase name of a dialect, e.g. `liquid`, is what `--dialect` accepts.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Display,
    EnumString,
    IntoStaticStr,
    VariantArray,
    Deserialize,
)]
#[strum(serialize_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Dialect {
    /// Django templates, used when nothing else identifies the dialect.
    ///
    /// Reference: https://docs.djangoproject.com/en/stable/ref/templates/language/
    #[default]
    Django,
    /// Reference: https://jinja.palletsprojects.com/en/stable/templates/
    Jinja,
    /// Reference: https://shopify.github.io/liquid/
    Liquid,
    /// Reference: https://twig.symfony.com/doc/3.x/templates.html
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
    ///
    /// Reference:
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/language/#comments
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#comments
    /// - Twig: https://twig.symfony.com/doc/3.x/templates.html#comments
    pub fn has_hash_comments(self) -> bool {
        matches!(self, Dialect::Django | Dialect::Jinja | Dialect::Twig)
    }

    /// Whether `{% raw %}` blocks exist, whose contents are output as they are
    /// written. Django and Twig have `{% verbatim %}` instead.
    ///
    /// Reference:
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#escaping
    /// - Liquid: https://shopify.github.io/liquid/tags/template/#raw
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#verbatim
    /// - Twig: https://twig.symfony.com/doc/3.x/tags/verbatim.html
    pub fn has_raw_blocks(self) -> bool {
        matches!(self, Dialect::Jinja | Dialect::Liquid)
    }

    /// Whether a `{% %}` tag that starts with `#` is a comment, e.g.
    /// `{% # note %}`.
    ///
    /// Reference: https://shopify.github.io/liquid/tags/template/#inline-comments
    pub fn has_inline_comments(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether the `{% liquid %}` tag exists, which holds a tag on each of its
    /// lines, without their delimiters.
    ///
    /// Reference: https://shopify.github.io/liquid/tags/template/#liquid
    pub fn has_liquid_tag(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a name can end with a `?`, as Shopify's properties do, e.g.
    /// `product.gift_card?`. Liquid's lexer reads a name as
    /// `[a-zA-Z_][\w-]*\??`.
    ///
    /// Reference: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/lexer.rb#L23
    pub fn names_can_end_with_question_mark(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// The markers that control the whitespace around a tag when they are
    /// written next to its delimiters, e.g. the `-` in `{%- if x -%}`. Django
    /// has none.
    ///
    /// Reference:
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#whitespace-control
    /// - Liquid: https://shopify.github.io/liquid/basics/whitespace/
    /// - Twig: https://twig.symfony.com/doc/3.x/templates.html#whitespace-control
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
    ///
    /// Reference:
    /// - Liquid: https://github.com/Shopify/liquid/blob/v5.14.0/lib/liquid/variable.rb#L17
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/language/#filters
    pub fn filter_supports_arg_list(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a tag's arguments are separated by commas, e.g.
    /// `{% cycle "a", "b" %}` or `{% include "card", product: product %}`.
    ///
    /// Reference:
    /// - https://shopify.github.io/liquid/tags/iteration/#cycle
    /// - https://shopify.github.io/liquid/tags/template/#render-parameters
    pub fn requires_comma_separated_args(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a tag's named arguments are written `name: value`, rather than
    /// `name=value`.
    ///
    /// Reference:
    /// - Liquid: https://shopify.github.io/liquid/tags/template/#render-parameters
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#include
    pub fn named_args_use_colon(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a range of integers can be written `(1..n)`.
    ///
    /// Reference: https://shopify.github.io/liquid/tags/iteration/#range
    pub fn has_ranges(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether `nil`, `null`, `blank` and `empty` are literals, along with
    /// `true` and `false`, e.g. `{% if items == empty %}`. The other dialects
    /// read them as names like any other.
    ///
    /// Reference:
    /// - https://shopify.github.io/liquid/basics/types/#nil
    /// - https://shopify.github.io/liquid/basics/types/#emptydrop
    pub fn has_nil_and_empty_literals(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a loop takes parameters after its iterator, which can be
    /// separated by commas, e.g. `{% for item in items limit: 2, offset: 1 %}`.
    ///
    /// Reference: https://shopify.github.io/liquid/tags/iteration/#for-parameters
    pub fn has_loop_params(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether an assignment can be written `value as name`, e.g.
    /// `{% with business.employees.count as total %}`, as well as
    /// `name=value`.
    ///
    /// Reference: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#with
    pub fn has_as_assignments(self) -> bool {
        matches!(self, Dialect::Django)
    }

    /// How the dialect spells `elif`, which is `elsif` in Liquid.
    ///
    /// Reference:
    /// - Liquid: https://shopify.github.io/liquid/tags/control-flow/#elsif--else
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#if
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#if
    /// - Twig: https://twig.symfony.com/doc/3.x/tags/if.html
    pub fn elif_tag(self) -> &'static str {
        if self.is_liquid() { "elsif" } else { "elif" }
    }

    /// The tag that starts the body of a loop for when there is nothing to
    /// loop over, which is `empty` in Django and `else` in the other dialects.
    ///
    /// Reference:
    /// - Django: https://docs.djangoproject.com/en/stable/ref/templates/builtins/#for-empty
    /// - Jinja: https://jinja.palletsprojects.com/en/stable/templates/#for
    /// - Liquid: https://shopify.github.io/liquid/tags/iteration/#else
    /// - Twig: https://twig.symfony.com/doc/3.x/tags/for.html#the-else-clause
    pub fn empty_loop_tag(self) -> &'static str {
        if self.is_django() { "empty" } else { "else" }
    }

    /// The keyword that the identifier `name` is in the dialect, if any.
    /// Liquid has keywords of its own, and spells `elif` as `elsif`.
    pub fn keyword(self, name: &str) -> Option<Keyword> {
        if name == self.elif_tag() {
            return Some(Keyword::Elif);
        }

        match self {
            Dialect::Liquid if name == "elif" => None,
            Dialect::Liquid => LIQUID_KEYWORDS.get(name).or_else(|| KEYWORDS.get(name)).copied(),
            _ => KEYWORDS.get(name).copied(),
        }
    }

    /// How tightly the binary operator `op` binds, as the binding powers of
    /// its left and right side, or `None` if the dialect has no such operator.
    /// An operator with a higher binding power groups first, so `a or b and c`
    /// is `a or (b and c)`.
    ///
    /// Each dialect follows its own engine, see the comment on each arm. Only
    /// Jinja and Twig have arithmetic and `~`, and only Liquid has `contains`.
    pub fn infix_binding_power(self, op: BinOp) -> Option<(u8, u8)> {
        let power = match self {
            // Django's own precedences, from `smartif.py`. `in` and `not in`
            // bind looser than the other comparisons, so `a in b == c` is
            // `a in (b == c)`.
            // Reference: https://github.com/django/django/blob/5.2.18/django/template/smartif.py#L97-L111
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
                | BinOp::IsNot => left_assoc(10),
                BinOp::Contains
                | BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::FloorDiv
                | BinOp::Mod
                | BinOp::Pow
                | BinOp::Concat => return None,
            },

            // Jinja's parser has a function for each level, from `parse_or`
            // to `parse_pow`, rather than numbers, so these go up in tens.
            // Jinja chains comparisons like Python, so `a == b > c` means
            // `a == b and b > c`, which is `(a == b) > c` here. Unlike in
            // Python, `**` is left-associative, so `a ** b ** c` is
            // `(a ** b) ** c`. A test binds tighter than any operator.
            // Reference: https://github.com/pallets/jinja/blob/3.1.6/src/jinja2/parser.py#L536-L624
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
                | BinOp::NotIn => left_assoc(40),
                BinOp::Add | BinOp::Sub => left_assoc(50),
                BinOp::Concat => left_assoc(60),
                BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => left_assoc(70),
                BinOp::Pow => left_assoc(80),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
                BinOp::Contains => return None,
            },

            // Liquid has no precedence between `and` and `or`, and evaluates
            // them from right to left, so `a and b or c` is `a and (b or c)`.
            // Liquid does arithmetic with filters such as `plus` instead.
            // Reference: https://shopify.github.io/liquid/basics/operators/#order-of-operations
            Dialect::Liquid => match op {
                BinOp::And | BinOp::Or => right_assoc(2),
                BinOp::In | BinOp::NotIn => left_assoc(8),
                BinOp::Eq | BinOp::NotEq | BinOp::Is | BinOp::IsNot => right_assoc(8),
                BinOp::Gt | BinOp::GtEq | BinOp::Lt | BinOp::LtEq | BinOp::Contains => {
                    left_assoc(9)
                }
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::FloorDiv
                | BinOp::Mod
                | BinOp::Pow
                | BinOp::Concat => return None,
            },

            // Twig's own precedences, from `CoreExtension`. A test binds
            // tighter than the comparisons, `not` and the arithmetic below
            // `**`. Twig 3.15 deprecates `~` at 40 in favour of 27, below `+`
            // and `-`, but 40 is what it still parses with.
            // Reference: https://github.com/twigphp/Twig/blob/v3.30.0/src/Extension/CoreExtension.php#L343-L391
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
                | BinOp::NotIn => left_assoc(20),
                BinOp::Add | BinOp::Sub => left_assoc(30),
                BinOp::Concat => left_assoc(40),
                BinOp::Mul | BinOp::Div | BinOp::FloorDiv | BinOp::Mod => left_assoc(60),
                BinOp::Is | BinOp::IsNot => left_assoc(100),
                BinOp::Pow => right_assoc(200),
                BinOp::Contains => return None,
            },
        };

        Some(power)
    }

    /// How tightly the prefix operator `op` binds: its operand takes every
    /// binary operator whose left binding power is at least this, see
    /// [Dialect::infix_binding_power]. `-` binds tighter than any binary
    /// operator, including `**`, so `-a ** b` is `(-a) ** b`.
    ///
    /// `not` binds tighter than `and` and `or` in every dialect. In Twig it
    /// also binds tighter than the comparisons, `+`, `-` and `~`, so
    /// `not a == b` is `(not a) == b` there and `not (a == b)` in the other
    /// dialects.
    ///
    /// Reference: the sources on each arm of [Dialect::infix_binding_power].
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

    #[test]
    fn test_keyword() {
        assert_eq!(Dialect::Django.keyword("for"), Some(Keyword::For));
        assert_eq!(Dialect::Liquid.keyword("for"), Some(Keyword::For));
        assert_eq!(Dialect::Twig.keyword("user"), None);

        assert_eq!(Dialect::Jinja.keyword("elif"), Some(Keyword::Elif));
        assert_eq!(Dialect::Jinja.keyword("elsif"), None);
        assert_eq!(Dialect::Liquid.keyword("elsif"), Some(Keyword::Elif));
        assert_eq!(Dialect::Liquid.keyword("elif"), None);

        assert_eq!(Dialect::Liquid.keyword("unless"), Some(Keyword::Unless));
        assert_eq!(Dialect::Django.keyword("unless"), None);
    }

    #[test]
    fn test_operators_of_each_dialect() {
        let arithmetic = [
            BinOp::Add,
            BinOp::Sub,
            BinOp::Mul,
            BinOp::Div,
            BinOp::FloorDiv,
            BinOp::Mod,
            BinOp::Pow,
            BinOp::Concat,
        ];

        for &dialect in Dialect::ALL {
            let has_arithmetic = matches!(dialect, Dialect::Jinja | Dialect::Twig);
            for op in arithmetic {
                let power = dialect.infix_binding_power(op);
                assert_eq!(power.is_some(), has_arithmetic, "`{op}` in {dialect}");
            }

            let power = dialect.infix_binding_power(BinOp::Contains);
            assert_eq!(power.is_some(), dialect.is_liquid(), "`contains` in {dialect}");
        }
    }
}
