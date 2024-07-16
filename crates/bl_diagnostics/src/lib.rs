//! Definitions for all of the diagnostics that are available to the user.

pub enum DiagnosticKind {
    Error,
    Warning,
    Note,
}

pub struct Diagnostic {
    pub kind: DiagnosticKind,
}

#[derive(Default)]
pub struct Diagnostics(pub Vec<Diagnostic>);
