//! Hash Parser diagnostic utilities, error and warning definitions.
//! This module contains all of the logic that provides diagnostic
//! capabilities within the parser.
pub(crate) mod error;
pub(crate) mod expected;
pub(crate) mod warning;

use bl_reporting::{DiagnosticStore, DiagnosticsMut, HasDiagnosticsMut, Reports};
use bl_workspace::{Member, settings::DialectSource};

use self::{error::ParseError, warning::ParseWarning};
use crate::parser::Parser;

impl HasDiagnosticsMut for Parser<'_, '_> {
    type Diagnostics = ParserDiagnostics;

    fn diagnostics(&mut self) -> &mut Self::Diagnostics {
        self.diagnostics
    }

    fn add_error(&mut self, error: <Self::Diagnostics as DiagnosticsMut>::Error) {
        // Specify that an error occurred in the current frame, so that the
        // frame exit handler can avoid emitting an `expected_eof`.
        //
        // self.frame.error.set(true);
        //
        self.diagnostics().add_error(error);
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

/// Shorthand for the parser diagnostics.
pub type ParserDiagnostics = DiagnosticStore<ParseError, ParseWarning>;

/// Add a note to each of `reports`, from lexing and parsing `member`, that says
/// which dialect it is parsed in, and how it is picked. A template can read as
/// another dialect's, so a diagnostic might only make sense in a dialect that
/// it isn't parsed in. The note comes after the report's helps, which matter
/// more.
pub(crate) fn note_dialect(reports: &mut Reports, member: &Member) {
    if reports.is_empty() {
        return;
    }

    let dialect = member.dialect;
    let source = match (member.dialect_source, dialect.extension()) {
        (DialectSource::Flag, _) => format!("from `--dialect {}`", dialect.name()),
        (DialectSource::Extension, Some(extension)) => format!("from its `.{extension}` extension"),
        (DialectSource::Project, _) => "from the `dialect` in `bracketlint.toml`".to_string(),
        _ => "the default, since neither `--dialect`, its extension nor `bracketlint.toml` names \
              a dialect"
            .to_string(),
    };
    let message = format!("this file is parsed as {}, {source}", dialect.title());

    for report in reports {
        report.add_note(&message);
    }
}
