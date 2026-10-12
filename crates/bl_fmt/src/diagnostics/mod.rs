//! Diagnostics module for the formatter.

mod error;
mod warning;

use bl_reporting::DiagnosticStore;
pub use error::{FmtError, FmtErrorKind};
pub use warning::{FmtWarning, FmtWarningKind};

pub type FmtResult<T> = Result<T, FmtError>;

/// What the formatter reports while it formats all of a template, e.g. a
/// `<script>` whose code doesn't parse.
pub type FmtDiagnostics = DiagnosticStore<FmtError, FmtWarning>;
