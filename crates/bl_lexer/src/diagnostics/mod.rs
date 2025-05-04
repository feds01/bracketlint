use bl_ast::Span;
use bl_reporting::{DiagnosticStore, ReportBuilder, Reports};

use crate::token::Delimiter;

#[derive(Debug, Clone, Copy)]
pub enum LexerErrorKind {
    /// When a string literal is considered to be unclosed.
    UnclosedStringLit,

    /// When a float literal specifies an exponent, but no digits are provided.
    /// i.e. ```
    /// 1.0e
    /// ```
    MissingExponentDigits,

    /// When a float literal specifies an invalid exponent. i.e.
    /// ```ignore
    /// 1.0e-1.0
    /// ```
    InvalidFloatExponent,

    /// When a token tree is left un-closed without a matching delimiter.
    Unclosed(Delimiter),
}

/// The error type for the lexer.
#[derive(Debug, Clone, Copy)]
pub struct LexerError {
    /// The kind of the error.
    pub kind: LexerErrorKind,

    /// The span of the error.
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
            LexerErrorKind::Unclosed(delim) => format!(
                "encountered unclosed delimiter `{}`, add a `{}` after the inner expression",
                delim.left(),
                delim.right()
            ),
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

/// Warning types that can occur from the lexer.
#[derive(Debug, Clone, Copy)]
pub enum LexerWarningKind {}

/// The warning type for the lexer.
#[derive(Debug, Clone, Copy)]
pub struct LexerWarning {
    /// The kind of the warning.
    pub kind: LexerWarningKind,

    /// The span of the warning.
    pub span: Span,
}

impl From<LexerWarning> for Reports {
    fn from(_: LexerWarning) -> Self {
        todo!()
    }
}

/// The diagnostics store for the lexer.
pub type LexerDiagnostics = DiagnosticStore<LexerError, LexerWarning>;

/// The result type for the lexer.
pub type LexerResult<T> = Result<T, LexerError>;
