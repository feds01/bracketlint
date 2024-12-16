//! Any parser errors that the parser can emit and report.

use bl_reporting::Reports;

#[derive(Clone)]
pub enum ParseError {}

impl From<ParseError> for Reports {
    fn from(_: ParseError) -> Self {
        todo!()
    }
}
