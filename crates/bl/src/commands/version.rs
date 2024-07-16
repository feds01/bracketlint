//! `version` command implemention for the CLI.

use anyhow::Result;

pub fn version() -> Result<()> {
    let version_info = crate::version::version();
    println!("bl {}", version_info);

    Ok(())
}
