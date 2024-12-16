use bl_reporting::{store::DiagnosticStore, Reports};

/// The error type for the lexer.
#[derive(Debug, Clone, Copy)]
pub enum LexerError {}

impl From<LexerError> for Reports {
    fn from(_: LexerError) -> Self {
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum LexerWarning {}

impl From<LexerWarning> for Reports {
    fn from(_: LexerWarning) -> Self {
        todo!()
    }
}

pub type LexerDiagnostics = DiagnosticStore<LexerError, LexerWarning>;
