//! Definitions of the template dialects that bracketlint understands.

use std::path::Path;

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

    /// Check if its currently Django dialect.
    #[inline]
    pub fn is_django(self) -> bool {
        self == Dialect::Django
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
}
