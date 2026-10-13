//! Utilities for showing the version of the tool.

use std::fmt;

pub struct VersionInfo {}

impl fmt::Display for VersionInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The version in `crates/bl/Cargo.toml`, which `--version` prints too.
        write!(f, "{}", env!("CARGO_PKG_VERSION"))
    }
}

pub fn version() -> VersionInfo {
    VersionInfo {}
}
