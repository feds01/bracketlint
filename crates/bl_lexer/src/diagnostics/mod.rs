use bl_ast::Span;
use bl_reporting::{DiagnosticStore, ReportBuilder, Reports};

#[derive(Debug, Clone, Copy)]
pub enum LexerErrorKind {
    /// When a string literal is considered to be unclosed.
    UnclosedStringLit,

    MissingExponentDigits,

    InvalidFloatExponent,
}

/// The error type for the lexer.
#[derive(Debug, Clone, Copy)]
pub struct LexerError {
    pub kind: LexerErrorKind,
    pub span: Span,
}

impl From<LexerError> for Reports {
    fn from(err: LexerError) -> Self {
        let mut reporter = ReportBuilder::default();

        let help_notes = vec![];

        let message = match err.kind {
            LexerErrorKind::MissingExponentDigits => {
                "float exponent to have at least one digit".to_string()
            }
            LexerErrorKind::UnclosedStringLit => "unclosed string literal".to_string(),
            LexerErrorKind::InvalidFloatExponent => {
                "float literal has an invalid exponent".to_string()
            }
        };

        let report = reporter.error().title(message).add_span(err.span);

        // Add any of the additionally generated notes.
        for note in help_notes {
            report.add_element(note);
        }

        reporter.into_reports()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LexerWarning {}

impl From<LexerWarning> for Reports {
    fn from(_: LexerWarning) -> Self {
        todo!()
    }
}

pub type LexerDiagnostics = DiagnosticStore<LexerError, LexerWarning>;

pub type LexerResult<T> = Result<T, LexerError>;
