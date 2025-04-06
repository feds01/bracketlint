#![allow(dead_code)]

use core::fmt;

use bl_ast::SourceId;
use derive_more::Constructor;

use crate::diagnostics::FmtResult;

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

pub struct TerminalState {
    pub language: LanguageType,
}

#[derive(Debug, Clone, Constructor)]
pub struct FormatterContext {
    /// The source of the module that is being formatted. This is mostly
    /// useful for diagnostics.
    pub source: SourceId,

    /// The indent level.
    pub indent: u16,
}

impl FormatterContext {
    /// Get the ID of the source that is being formatted.
    #[inline(always)]
    pub fn source(&self) -> SourceId {
        self.source
    }

    /// Get the current indent level.
    pub fn indent(&self) -> u16 {
        self.indent
    }
}

/// A trait that represents a type that has the capability to parse HTML.
pub trait HasHTMLParsing {
    fn format(&mut self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

/// A trait that represents a type that has the capability to parse CSS.
pub trait HasCSSParsing {
    fn format(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

/// A trait that represents a type that has the capability to parse JavaScript.
pub trait HasJSParsing {
    fn format(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

pub(crate) trait ExternalLanguagesEngineAdaptor {
    type HTMLEngine: HasHTMLParsing;
    type CSSEngine: HasCSSParsing;
    type JSEngine: HasJSParsing;

    /// A constructor for running the HTML formatting engine.
    fn html_engine(&self, context: &FormatterContext) -> Self::HTMLEngine;

    fn css_engine(&self, context: &FormatterContext) -> Self::CSSEngine;

    fn js_engine(&self, context: &FormatterContext) -> Self::JSEngine;
}
