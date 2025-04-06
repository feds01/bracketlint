//! Diagnostics module for the formatter.

mod error;
mod warning;

pub use error::{FmtError, FmtErrorKind};

pub type FmtResult<T> = Result<T, FmtError>;
