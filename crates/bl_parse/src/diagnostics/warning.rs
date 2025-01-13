//! Any warning that the parser can emit and report.

use bl_ast::Span;
use bl_reporting::{ReportBuilder, Reports};

#[derive(Debug, Clone, Copy)]
pub struct ParseWarning {
    kind: ParseWarningKind,
    span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum ParseWarningKind {
    /// A block label is a string literal instead of being an identifier.
    ///
    /// Whilst this is valid in some dialects, it is not valid in others.
    BlockLabelIsStringLiteral,
}

impl ParseWarning {
    pub fn new(kind: ParseWarningKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl From<ParseWarning> for Reports {
    fn from(warning: ParseWarning) -> Self {
        let ParseWarning { kind, span } = warning;

        let mut reporter = ReportBuilder::default();
        let report = reporter.warning();

        match kind {
            ParseWarningKind::BlockLabelIsStringLiteral => {
                report
                    .title("block label is a string literal")
                    .add_labelled_span(span, "block label is a string literal");
            }
        }

        reporter.into_reports()
    }
}
