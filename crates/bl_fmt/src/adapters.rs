#![allow(dead_code)]

use core::fmt;

use bl_ast::{SourceId, SpannedSource};
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
    fn format(&mut self, contents: &str) -> FmtResult<String>;

    /// Get the [TerminalState] of the parser.
    fn into_state(self) -> TerminalState;
}

/// A trait that represents a type that has the capability to parse CSS.
pub trait HasCSSParsing<'ctx> {
    fn format(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

/// A trait that represents a type that has the capability to parse JavaScript.
pub trait HasJSParsing<'ctx> {
    fn format(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
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
