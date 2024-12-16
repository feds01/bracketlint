//! Any warning that the parser can emit and report.

use bl_reporting::Reports;

#[derive(Clone)]
pub enum ParseWarning {}

impl From<ParseWarning> for Reports {
    fn from(_: ParseWarning) -> Self {
        todo!()
    }
}
