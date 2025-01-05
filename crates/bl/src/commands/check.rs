//! Implementation of the `check` command.

use std::{fs, path::PathBuf};

use anyhow::Result;
use bl_diagnostics::Diagnostics;
use bl_parse::{parse_source, ParseQuery, ParseResult};
use bl_reporting::Reporter;
use bl_utils::{stream::CompilerOutputStream, timed};
use bl_workspace::{resolver::find_files_in_paths, settings::Settings, WorkspaceBuilder};
use log::info;

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

    info!("found {} files", files.len());

    // @@Todo: move this out to above as this isn't the right place to
    // build the workspace.
    let builder = WorkspaceBuilder::new()
        .with_settings(settings)
        .with_stdout(CompilerOutputStream::stdout())
        .with_stderr(CompilerOutputStream::stderr());
    let mut workspace = builder.build();
    let renderer = default_renderer();

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.
    // Now iterate the files, parse them and lint them.
    //
    // @@Todo: we could fairly easily parallelize this operation, but we need to
    // first add `salsa` to the project. For now, we'll just lint them sequentially
    // and then parallelize it later.
    for file in files {
        match file {
            Ok(file) => {
                // We first need to convert this to a workspace member.
                let contents = fs::read_to_string(file.path())?;
                let id = workspace.members.reserve_member(file.into_path(), contents);
                let member = workspace.members.member(id);

                let ParseResult { node, diagnostics } =
                    parse_source(ParseQuery::new(id, &member.contents));

                // Print any shown diagnostics
                if !diagnostics.is_empty() {
                    for diagnostic in diagnostics.iter() {
                        let reporter = ReportWriter::new(&renderer, diagnostic);
                        println!("{}", reporter);
                    }
                }

                let member = workspace.members.member_mut(id);
                member.document = node;
            }
            Err(err) => {
                // @@todo: Create a diagnostic for the error, and print it!
                println!("error: {}", err);
            }
        }
    }

    Ok(Diagnostics::default())
}
