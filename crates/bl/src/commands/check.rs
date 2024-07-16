//! Implementation of the `check` command.

use std::path::PathBuf;

use anyhow::Result;
use bl_diagnostics::Diagnostics;
use bl_utils::timed;
use bl_workspace::{resolver::find_files_in_paths, settings::Settings};

pub fn check(files: &[PathBuf], settings: Settings) -> Result<Diagnostics> {
    // Firstly, we need to discover all of the files in the provided paths.
    let files = timed(
        || find_files_in_paths(files, &settings),
        log::Level::Info,
        |duration| println!("Resolved files in {:?}", duration),
    )?;

    if files.is_empty() {
        // @@Todo: warn the user that there were no files to check.
        return Ok(Diagnostics::default());
    }

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.

    // Now iterate the files in parallel, parse them and lint them.

    println!("found {} files", files.len());

    Ok(Diagnostics::default())
}
