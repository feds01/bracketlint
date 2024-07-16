//! Utilities for showing the version of the tool.

use std::fmt;

pub struct VersionInfo {}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // @@Todo: actually integrate this with the version information.
        write!(f, "0.1.0")
    }
}

pub fn version() -> VersionInfo {
    VersionInfo {}
}
