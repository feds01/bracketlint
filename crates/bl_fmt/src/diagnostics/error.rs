use bl_ast::Span;
use bl_reporting::{ReportBuilder, Reports};
use derive_more::Constructor;

use crate::adapters::LanguageType;

#[derive(Debug, Clone, Constructor)]
pub struct FmtError {
    /// The kind of error that was encountered.
    kind: FmtErrorKind,

    /// An optional location of where the external languages engine encountered
    /// a problem.
    span: Option<Span>,
}

impl FmtError {
    /// Create a compound error that contains multiple errors.
    pub fn compound(items: Vec<Self>) -> Self {
        Self { kind: FmtErrorKind::Compound { items }, span: None }
    }
}

#[derive(Debug, Clone)]
pub enum FmtErrorKind {
    /// An error occurred when parsing the external language. We defer the
    /// reporting of this error to the external language.
    ExternalLanguageParseError {
        language: LanguageType,
        message: String,
    },

    ExternalLanguageFormatError {
        language: LanguageType,
    },

    /// A compound error that contains multiple errors.
    Compound {
        items: Vec<FmtError>,
    },
}

impl From<FmtError> for Reports {
    fn from(value: FmtError) -> Self {
        let mut reporter = ReportBuilder::default();

        let (base_message, label) = match value.kind {
            FmtErrorKind::ExternalLanguageParseError { language, message } => {
                (format!("An error occurred when parsing the `{language}`."), Some(message))
            }
            FmtErrorKind::ExternalLanguageFormatError { language } => {
                (format!("An error occurred when formatting `{language}`"), None)
            }
            FmtErrorKind::Compound { items } => {
                let mut reports = Reports::new();
                for item in items {
                    reports.extend(Reports::from(item));
                }
                return reports;
            }
        };

        let report = reporter.error().title(base_message);

        match (label, value.span) {
            (Some(label), Some(span)) => report.add_labelled_span(span, label),
            (Some(label), None) => report.add_info(label),
            (None, Some(span)) => report.add_span(span),
            (None, None) => report,
        };

        reporter.into_reports()
    }
}
