//! The warnings that the formatter can report.

use bl_ast::Span;
use bl_reporting::{ReportBuilder, Reports};
use derive_more::Constructor;

use crate::adapters::LanguageType;

/// A warning that the formatter reports while it formats a template, which
/// it still formats all of.
#[derive(Debug, Clone, Constructor)]
pub struct FmtWarning {
    /// The kind of warning that was encountered.
    kind: FmtWarningKind,

    /// Where the warning is in the template.
    span: Span,
}

#[derive(Debug, Clone)]
pub enum FmtWarningKind {
    /// The code of a `<script>` or `<style>` that no template tag splits
    /// doesn't parse as `language`, for the reason `message`. The code is kept
    /// as it is written.
    UnparsedCode { language: LanguageType, message: String },
}

impl From<FmtWarning> for Reports {
    fn from(warning: FmtWarning) -> Self {
        let FmtWarning { kind, span } = warning;
        let mut reporter = ReportBuilder::default();

        match kind {
            FmtWarningKind::UnparsedCode { language, message } => {
                let title = match language.element() {
                    Some(element) => format!("the `<{element}>` doesn't parse as {language}"),
                    None => format!("the code doesn't parse as {language}"),
                };
                reporter
                    .warning()
                    .title(title)
                    .add_labelled_span(span, message)
                    .add_help("its code is kept as it is written");
            }
        }

        reporter.into_reports()
    }
}
