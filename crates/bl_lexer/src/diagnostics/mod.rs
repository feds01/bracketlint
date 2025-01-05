use bl_ast::Span;
use bl_reporting::{DiagnosticStore, ReportBuilder, Reports};

#[derive(Debug, Clone, Copy)]
pub enum LexerErrorKind {
}

/// The error type for the lexer.
#[derive(Debug, Clone, Copy)]
pub struct LexerError {
    pub kind: LexerErrorKind,
    pub span: Span,
}

impl From<LexerError> for Reports {
    fn from(_: LexerError) -> Self {
        todo!()
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
