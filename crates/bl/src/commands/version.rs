//! `version` command implementation for the CLI.

use anyhow::Result;

pub fn version() -> Result<()> {
    let version_info = crate::version::version();
    println!("bracketlint {}", version_info);

    Ok(())
}
