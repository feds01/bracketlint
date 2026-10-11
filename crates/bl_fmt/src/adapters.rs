#![allow(dead_code)]

use core::fmt;
use std::rc::Rc;

use bl_ast::{ByteRange, Dialect, SourceId, SpannedSource};
use derive_more::Constructor;

use crate::{
    diagnostics::FmtResult,
    dialects::html::{implied_ends, is_verbatim, keeps_whitespace},
    options::FormatterOptions,
};

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

    /// The names of the HTML elements that are open, outermost first, which
    /// the template opened in a piece of HTML before a tag and hasn't closed
    /// yet. Each indents what is inside it, as a block does. Each block keeps
    /// a copy, which shares the names.
    pub open_elements: Vec<Rc<str>>,
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

    /// Get the current indent level, of the blocks and the elements that are
    /// open.
    #[inline(always)]
    pub fn indent_level(&self) -> u16 {
        let depth = u16::try_from(self.open_elements.len()).unwrap_or(u16::MAX);
        self.state.indent.saturating_add(depth.saturating_mul(self.indent_step() as u16))
    }

    /// Whether the innermost open element is one whose content is kept as it
    /// is written, see [is_verbatim], e.g. a `<script>` that a tag splits.
    pub fn in_verbatim(&self) -> bool {
        self.open_elements.last().is_some_and(|name| is_verbatim(name))
    }

    /// Whether the innermost open element is one whose whitespace matters, see
    /// [keeps_whitespace].
    pub fn keeps_whitespace(&self) -> bool {
        self.open_elements.last().is_some_and(|name| keeps_whitespace(name))
    }

    /// Close the open element `name`, and any element opened inside it that
    /// is still open, as HTML does, e.g. an `<li>` at a `</ul>`. A closing tag
    /// of an element that isn't open closes nothing.
    pub(crate) fn close_element(&mut self, name: &str) {
        let open =
            self.open_elements.iter().rposition(|element| element.eq_ignore_ascii_case(name));
        if let Some(index) = open {
            self.open_elements.truncate(index);
        }

        self.state.language = self.element_language();
    }

    /// Open the elements `names`, outermost first, each closing the elements
    /// that it implies the end of, see [Self::close_implied].
    pub(crate) fn open_elements(&mut self, names: impl IntoIterator<Item = String>) {
        for name in names {
            self.close_implied(&name);
            self.open_elements.push(name.into());
        }

        self.state.language = self.element_language();
    }

    /// Close the open elements whose end an element `name` implies when it
    /// opens, see [implied_ends], e.g. the `<li>` before an `<li>`.
    pub(crate) fn close_implied(&mut self, name: &str) {
        let ends = implied_ends(name);
        while self
            .open_elements
            .last()
            .is_some_and(|open| ends.iter().any(|end| open.eq_ignore_ascii_case(end)))
        {
            self.open_elements.pop();
        }

        self.state.language = self.element_language();
    }

    /// Set the open elements to `elements`, e.g. those at the start of a block
    /// that each of its branches starts from.
    pub(crate) fn set_open_elements(&mut self, elements: Vec<Rc<str>>) {
        self.open_elements = elements;
        self.state.language = self.element_language();
    }

    /// The language of the text in the innermost open element, e.g. CSS in a
    /// `<style>` that a tag splits.
    fn element_language(&self) -> LanguageType {
        self.open_elements.last().map_or(LanguageType::Html, |name| LanguageType::of_element(name))
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

/// A part of a piece of HTML between template tags, see
/// [HasHTMLParsing::format].
#[derive(Debug)]
pub enum HtmlPart {
    /// The HTML at `range` in the piece, formatted, or `None` if it doesn't
    /// parse, in which case it is kept as it is written. It leaves the
    /// elements `opens` open, outermost first.
    Html { range: ByteRange, formatted: Option<String>, opens: Vec<String> },

    /// The content at `range` in the piece of one of the verbatim elements,
    /// see [is_verbatim], e.g. the code of a `<script>`, which is kept exactly
    /// as it is written.
    Verbatim { range: ByteRange },

    /// The closing tag at `range` in the piece of an element `name` that opens
    /// before the piece, e.g. the `</li>` in `{% endif %}</li>`.
    Close { range: ByteRange, name: String },
}

impl HtmlPart {
    /// The part, with its range moved by `offset`, e.g. from a slice of its
    /// piece to the piece.
    pub fn shift(self, offset: usize) -> Self {
        let shift = |range: ByteRange| ByteRange::new(range.start() + offset, range.end() + offset);
        match self {
            HtmlPart::Html { range, formatted, opens } => {
                HtmlPart::Html { range: shift(range), formatted, opens }
            }
            HtmlPart::Verbatim { range } => HtmlPart::Verbatim { range: shift(range) },
            HtmlPart::Close { range, name } => HtmlPart::Close { range: shift(range), name },
        }
    }
}

/// A trait that represents a type that has the capability to parse HTML.
pub trait HasHTMLParsing<'ctx> {
    /// Format `contents`, a piece of HTML between template tags, in its
    /// parts: the closing tags of elements that open before it, the content
    /// of the verbatim elements in it, see [is_verbatim], and the HTML between
    /// them. A part is never empty.
    fn format(&mut self, contents: &str) -> FmtResult<Vec<HtmlPart>>;
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
