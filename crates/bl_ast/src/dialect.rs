//! Definitions of the template dialects that bracketlint understands.

use std::path::Path;

use serde::Deserialize;
use strum::{Display, EnumString, IntoStaticStr, VariantArray};

use crate::keywords::{KEYWORDS, Keyword, LIQUID_KEYWORDS};

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

    /// Whether a `{% %}` tag that starts with `#` is a comment, e.g.
    /// `{% # note %}`.
    pub fn has_inline_comments(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether the `{% liquid %}` tag exists, which holds a tag on each of its
    /// lines, without their delimiters.
    pub fn has_liquid_tag(self) -> bool {
        matches!(self, Dialect::Liquid)
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

    /// Whether a range of integers can be written `(1..n)`.
    pub fn has_ranges(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether `nil`, `null`, `blank` and `empty` are literals, along with
    /// `true` and `false`, e.g. `{% if items == empty %}`. The other dialects
    /// read them as names like any other.
    pub fn has_nil_and_empty_literals(self) -> bool {
        matches!(self, Dialect::Liquid)
    }

    /// Whether a loop takes parameters after its iterator, which can be
    /// separated by commas, e.g. `{% for item in items limit: 2, offset: 1 %}`.
    pub fn has_loop_params(self) -> bool {
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
}
