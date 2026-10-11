#![allow(dead_code)]

use core::fmt;

use bl_ast::{Dialect, SourceId, SpannedSource};
use derive_more::Constructor;

use crate::{diagnostics::FmtResult, options::FormatterOptions};

/// The type of language that the code is written in. These can be encountered
/// when iterating through templates that are scanned by `brackelint`.
#[derive(Debug, Clone, Copy)]
pub enum LanguageType {
    /// HyperText Markup Language.
    Html,

    /// Cascading Style Sheets.
    Css,

    /// JavaScript or TypeScript
    Js,

    /// Unknown language.
    Text,
}

/// The elements whose content can't be laid out as other HTML is: whitespace
/// matters in `<pre>` and `<textarea>`, and `<script>` and `<style>` hold
/// code. Like any element name in HTML, they are matched regardless of case.
pub const VERBATIM_ELEMENTS: [&str; 4] = ["pre", "textarea", "script", "style"];

impl LanguageType {
    /// The element that holds code in the language within HTML, e.g. `style`
    /// for CSS, if the language is one that HTML embeds.
    pub fn element(self) -> Option<&'static str> {
        match self {
            LanguageType::Css => Some("style"),
            LanguageType::Js => Some("script"),
            LanguageType::Html | LanguageType::Text => None,
        }
    }

    /// The language of the content of the element `name`, e.g. CSS for a
    /// `<style>`, which is HTML unless the element holds code.
    pub fn of_element(name: &str) -> Self {
        for language in [LanguageType::Css, LanguageType::Js] {
            if language.element().is_some_and(|element| name.eq_ignore_ascii_case(element)) {
                return language;
            }
        }

        LanguageType::Html
    }

    /// Where the code in the language ends in `text`, i.e. the closing tag of
    /// its [element](Self::element), e.g. `</style>` for CSS.
    pub fn end_in(self, text: &str) -> Option<usize> {
        let element = self.element()?;

        for (start, _) in text.match_indices("</") {
            let rest = &text[start + 2..];
            let Some(name) = rest.get(..element.len()) else {
                continue;
            };

            // The name ends at whitespace, `/` or `>`, so that e.g. `</scripts>`
            // is still code.
            let after = rest[element.len()..].chars().next();
            if name.eq_ignore_ascii_case(element)
                && after.is_none_or(|c| c.is_ascii_whitespace() || matches!(c, '/' | '>'))
            {
                return Some(start);
            }
        }

        None
    }
}

impl fmt::Display for LanguageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LanguageType::Html => write!(f, "HTML"),
            LanguageType::Css => write!(f, "CSS"),
            LanguageType::Js => write!(f, "JavaScript"),
            LanguageType::Text => write!(f, "Text"),
        }
    }
}

/// The state of the terminal. This is used to determine the language that
#[derive(Debug, Clone, Copy)]
pub struct TerminalState {
    pub language: LanguageType,
    pub indent: u16,
}

#[derive(Debug, Clone, Constructor)]
pub struct FormatterContext<'s> {
    pub id: SourceId,

    /// The source of the module that is being formatted. This is mostly
    /// useful for diagnostics.
    pub source: SpannedSource<'s>,

    pub options: FormatterOptions,

    /// The template dialect of the module, which decides how some tags are
    /// spelled, e.g. `elsif` in Liquid and `elif` elsewhere.
    pub dialect: Dialect,

    /// We should be storing the last [`TerminalState`] that was
    /// encountered. This is used to determine the language that
    /// the code is written in.
    pub state: TerminalState,
}

impl<'s> FormatterContext<'s> {
    /// Get the ID of the source that is being formatted.
    #[inline(always)]
    pub fn id(&self) -> SourceId {
        self.id
    }

    pub fn source(&self) -> SpannedSource<'_> {
        self.source
    }

    /// Get the current language of the parser.
    #[inline(always)]
    pub fn language(&self) -> LanguageType {
        self.state.language
    }

    /// Get the current indent level.
    #[inline(always)]
    pub fn indent_level(&self) -> u16 {
        self.state.indent
    }

    pub fn indent_step(&self) -> u8 {
        self.options.indent_size
    }

    /// Decrease the indent level by the indent step.
    pub(crate) fn decrement_indent(&mut self) {
        self.state.indent = self.state.indent.saturating_sub(self.indent_step() as u16);
    }

    /// Increase the indent level by the indent step.
    pub(crate) fn increment_indent(&mut self) {
        self.state.indent += self.indent_step() as u16;
    }
}

/// A trait that represents a type that has the capability to parse HTML.
pub trait HasHTMLParsing<'ctx> {
    /// Format `contents`, or return `None` if they don't parse, in which case
    /// they are kept as they are written.
    fn format(&mut self, contents: &str) -> FmtResult<Option<String>>;

    /// Get the [TerminalState] of the parser.
    fn into_state(self) -> TerminalState;
}

/// A trait that represents a type that has the capability to parse CSS.
pub trait HasCSSParsing<'ctx> {
    /// Format `contents`, or return `None` if they don't parse, in which case
    /// they are kept as they are written.
    fn format(&self, contents: &str) -> FmtResult<Option<String>>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn into_state(self) -> TerminalState;
}

/// A trait that represents a type that has the capability to parse JavaScript.
pub trait HasJSParsing<'ctx> {
    /// Format `contents`, or return `None` if they don't parse, in which case
    /// they are kept as they are written.
    fn format(&self, contents: &str) -> FmtResult<Option<String>>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn into_state(self) -> TerminalState;
}

pub(crate) trait ExternalLanguagesEngineAdaptor {
    type HTMLEngine<'ctx>: HasHTMLParsing<'ctx>;
    type CSSEngine<'ctx>: HasCSSParsing<'ctx>;
    type JSEngine<'ctx>: HasJSParsing<'ctx>;

    /// A constructor for running the HTML formatting engine.
    fn html_engine<'ctx>(&self, context: &'ctx FormatterContext<'_>) -> Self::HTMLEngine<'ctx>;

    fn css_engine<'ctx>(&self, context: &'ctx FormatterContext<'_>) -> Self::CSSEngine<'ctx>;

    fn js_engine<'ctx>(&self, context: &'ctx FormatterContext<'_>) -> Self::JSEngine<'ctx>;
}

#[cfg(test)]
mod tests {
    use super::LanguageType;

    #[test]
    fn code_ends_at_the_closing_tag_of_its_element() {
        assert_eq!(LanguageType::Js.end_in("f();\n</script>\n<p>"), Some(5));
        assert_eq!(LanguageType::Css.end_in("p {}</STYLE >"), Some(4));
        assert_eq!(LanguageType::Js.end_in("a < b; '</scripts>'; </script/>"), Some(21));
        assert_eq!(LanguageType::Js.end_in("f(); </style>"), None);
        assert_eq!(LanguageType::Html.end_in("</p>"), None);
    }

    #[test]
    fn elements_hold_code_regardless_of_case() {
        assert!(matches!(LanguageType::of_element("SCRIPT"), LanguageType::Js));
        assert!(matches!(LanguageType::of_element("style"), LanguageType::Css));
        assert!(matches!(LanguageType::of_element("pre"), LanguageType::Html));
    }
}
