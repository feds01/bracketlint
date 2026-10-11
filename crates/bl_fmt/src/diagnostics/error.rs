use bl_reporting::{ReportBuilder, Reports};
use derive_more::Constructor;

use crate::adapters::LanguageType;

#[derive(Debug, Clone, Constructor)]
pub struct FmtError {
    /// The kind of error that was encountered.
    kind: FmtErrorKind,
}

#[derive(Debug, Clone)]
pub enum FmtErrorKind {
    /// The external language parsed, but its formatter failed.
    ExternalLanguageFormatError { language: LanguageType },
}

impl From<FmtError> for Reports {
    fn from(value: FmtError) -> Self {
        let mut reporter = ReportBuilder::default();

        let base_message = match value.kind {
            FmtErrorKind::ExternalLanguageFormatError { language } => {
                format!("An error occurred when formatting `{language}`")
            }
        };

        reporter.error().title(base_message);
        reporter.into_reports()
    }
}
