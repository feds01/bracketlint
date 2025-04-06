#![allow(dead_code)]

use core::fmt;

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

/// A trait that represents a type that has the capability to parse HTML.
pub trait HasHtmlParsing {
    fn format_html(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

/// A trait that represents a type that has the capability to parse CSS.
pub trait HasCssParsing {
    fn format_css(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

/// A trait that represents a type that has the capability to parse JavaScript.
pub trait HasJsParsing {
    fn format_js(&self, contents: &str) -> FmtResult<String>;

    /// Attempt to compute the [TerminalState] of the parser.
    fn terminal_state(&self) -> Option<TerminalState>;
}

pub trait ExternalLanguagesEngine: HasCssParsing + HasHtmlParsing + HasJsParsing {}
