//! Utilities and wrapper functions for the reporting mechanism
//! within `bl`. This provides parts of `bl` to create, manage, and
//! emit arbitrary diagnostics to the user.
#![feature(decl_macro)]

pub mod store;
pub mod utils;

use std::fmt;

use bl_ast::Span;
use schemars::{self, JsonSchema};
use serde::{self, Serialize};

/// An alias for a collection of [Report]s.
pub type Reports = Vec<Report>;

/// Enumeration describing the kind of [Report]; either being a warning, info or
/// an error.
#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq, JsonSchema, Serialize)]
#[serde(crate = "self::serde")]
pub enum ReportKind {
    /// The report is an error.
    Error,
    /// The report is an informational diagnostic (likely for internal
    /// purposes).
    Info,
    /// The report is a warning.
    Warning,
    // This is an internal compiler error.
    Internal,
}

/// The kind of [ReportNote], this is primarily used for rendering the label of
/// the [ReportNote].
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema, Serialize)]
#[serde(crate = "self::serde")]
pub enum ReportNoteKind {
    /// A help message or a suggestion.
    Help,

    /// Information note
    Info,

    /// Additional information about the diagnostic.
    Note,
}

impl ReportNoteKind {
    /// Get the string representation of the label.
    pub fn as_str(&self) -> &'static str {
        match self {
            ReportNoteKind::Note => "note",
            ReportNoteKind::Info => "info",
            ReportNoteKind::Help => "help",
        }
    }
}

/// Data type representing a report note which consists of a label and the
/// message.
#[derive(Debug, Clone, JsonSchema, Serialize)]
#[serde(crate = "self::serde")]
pub struct ReportNote {
    /// The severity of the note.
    pub label: ReportNoteKind,

    /// The message associated with the note.
    pub message: String,
}

impl ReportNote {
    pub fn new(label: ReportNoteKind, message: impl ToString) -> Self {
        Self { label, message: message.to_string() }
    }
}

/// Data structure representing an associated block of code with a report. The
/// type contains the span of the block, the message associated with a block and
/// optional [ReportCodeBlockInfo] which adds a message pointed to a code item.
#[derive(Debug, Clone, JsonSchema, Serialize)]
#[serde(crate = "self::serde")]
pub struct ReportCodeBlock {
    /// The span and message pairs.
    pub notes: Vec<(Span, String)>,
}

impl ReportCodeBlock {
    /// Create a new [ReportCodeBlock] from a [Span] and a message.
    pub fn new(source_location: Span, code_message: impl ToString) -> Self {
        Self { notes: vec![(source_location, code_message.to_string())] }
    }
}

/// Enumeration representing types of components of a [Report]. A [Report] can
/// be made of either [ReportCodeBlock]s or [ReportNote]s.
#[derive(Debug, Clone, JsonSchema, Serialize)]
#[serde(crate = "self::serde")]
pub enum ReportElement {
    /// A note with code block.
    CodeBlock(ReportCodeBlock),

    /// A note on a report, without an associated [Span].
    Note(ReportNote),
}

/// Create a `help` note with the given message.
pub macro help {
    ($($arg:tt)*) => {
        ReportElement::Note(ReportNote::new(ReportNoteKind::Help, format!($($arg)*)))
    }
}

/// Create a `note` note with the given message.
pub macro note {
    ($($arg:tt)*) => {
        ReportElement::Note(ReportNote::new(ReportNoteKind::Note, format!($($arg)*)))
    }
}

/// Create a `info` note with the given message.
pub macro info {
    ($($arg:tt)*) => {
        ReportElement::Note(ReportNote::new(ReportNoteKind::Info, format!($($arg)*)))
    }
}

/// The report data type represents the entire report which might contain many
/// [ReportElement]s. The report also contains a general [ReportKind] and a
/// general message.
#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(crate = "self::serde")]
pub struct Report {
    /// The general kind of the report.
    pub kind: ReportKind,
    /// A title for the report.
    pub title: String,
    /// An optional associated general error code with the report.
    ///
    /// @@Dumbness: has to be a string because the `annotate-snippets`
    /// has lifetime problems.
    pub error_code: Option<String>,
    /// A vector of additional [ReportElement]s in order to add additional
    /// context to errors.
    pub contents: Vec<ReportElement>,
}

impl Report {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the report denotes an occurred error.
    pub fn is_error(&self) -> bool {
        self.kind == ReportKind::Error
    }

    /// Check if the report denotes an occurred warning.
    pub fn is_warning(&self) -> bool {
        self.kind == ReportKind::Warning
    }

    /// Add a title to the [Report].
    pub fn title(&mut self, title: impl ToString) -> &mut Self {
        self.title = title.to_string();
        self
    }

    /// Add a general kind to the [Report].
    pub fn kind(&mut self, kind: ReportKind) -> &mut Self {
        self.kind = kind;
        self
    }

    /// Add an associated [HashErrorCode] to the [Report].
    pub fn code(&mut self, error_code: usize) -> &mut Self {
        self.error_code = Some(error_code.to_string());
        self
    }

    /// Add a [`ReportNoteKind::Help`] note with the given message to the
    /// [Report].
    pub fn add_help(&mut self, message: impl ToString) -> &mut Self {
        self.add_element(ReportElement::Note(ReportNote::new(
            ReportNoteKind::Help,
            message.to_string(),
        )))
    }

    /// Add a [`ReportNoteKind::Info`] note with the given message to the
    /// [Report].
    pub fn add_info(&mut self, message: impl ToString) -> &mut Self {
        self.add_element(ReportElement::Note(ReportNote::new(
            ReportNoteKind::Info,
            message.to_string(),
        )))
    }

    /// Add a [`ReportNoteKind::Note`] note with the given message to the
    /// [Report].
    pub fn add_note(&mut self, message: impl ToString) -> &mut Self {
        self.add_element(ReportElement::Note(ReportNote::new(
            ReportNoteKind::Note,
            message.to_string(),
        )))
    }

    /// Add a code block at the given [Span] to the [Report].
    pub fn add_span(&mut self, location: Span) -> &mut Self {
        self.add_element(ReportElement::CodeBlock(ReportCodeBlock::new(location, "")))
    }

    /// Add a labelled code block at the given location to the [Report].
    pub fn add_labelled_span(&mut self, location: Span, message: impl ToString) -> &mut Self {
        self.add_element(ReportElement::CodeBlock(ReportCodeBlock::new(
            location,
            message.to_string(),
        )))
    }

    /// Add a [ReportElement] to the report.
    pub fn add_element(&mut self, element: ReportElement) -> &mut Self {
        self.contents.push(element);
        self
    }
}

impl Default for Report {
    fn default() -> Self {
        Self {
            kind: ReportKind::Error,
            title: "Bottom text".to_string(),
            error_code: None,
            contents: vec![],
        }
    }
}
