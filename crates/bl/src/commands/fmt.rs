//! Implementation of the `check` command.

use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use bl_fmt::{FmtOptions, FmtQuery, FmtQueryResult, fmt_module};
use bl_lints::{diff::Diff, settings};
use bl_parse::{ParseQuery, ParseQueryResult, parse_source};
use bl_reporting::Reports;
use bl_utils::{stream_writeln, timed};
use bl_workspace::{Workspace, resolver::find_files_in_paths};
use log::{Level, info};

pub fn fmt(files: &[PathBuf], workspace: &mut Workspace) -> Result<Reports> {
    // Firstly, we need to discover all of the files in the provided paths.
    let files = timed(
        || find_files_in_paths(files, &workspace.settings),
        log::Level::Info,
        |duration| info!("resolved files in {duration:?}"),
    )?;

    if files.is_empty() {
        // @@Todo: warn the user that there were no files to check.
        return Ok(Reports::default());
    }

    info!("found {} files", files.len());

    let mut pipeline_diagnostics = Reports::default();

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.
    // Now iterate the files, parse them and lint them.
    //
    // @@Todo: we could fairly easily parallelize this operation, but we need to
    // first add `salsa` to the project. For now, we'll just lint them sequentially
    // and then parallelize it later.
    timed(
        || {
            for file in files {
                match file {
                    Ok(file) => {
                        // We first need to convert this to a workspace member.
                        let Ok(contents) = fs::read_to_string(file.path()) else {
                            // @@Todo: add an error for the queue.
                            continue;
                        };

                        let id = workspace.members.reserve_member(file.into_path(), contents);
                        let member = workspace.members.member(id);

                        let ParseQueryResult { node, diagnostics } =
                            parse_source(ParseQuery::new(id, member));

                        pipeline_diagnostics.extend(diagnostics);

                        let member = workspace.members.member_mut(id);
                        member.document = node;
                    }
                    Err(err) => {
                        // @@todo: Create a diagnostic for the error, and print it!
                        println!("error: {}", err);
                    }
                }
            }
        },
        Level::Info,
        |duration| {
            info!("parsed files in {duration:?}");
        },
    );

    let options = FmtOptions::default();
    let settings = &workspace.settings;

    // Now let's try and "format" all of the documents that we got
    // in the project.
    //
    // @@Temp: for now we will just print the produced contents.
    for (source, member) in workspace.members.iter() {
        let FmtQueryResult { buffer, diagnostics } =
            fmt_module(FmtQuery { member, source, options });
        pipeline_diagnostics.extend(diagnostics);

        // @@Todo: factor this out into a general interface for emitting
        // changes/applying them.
        match settings.linter_settings.fix_mode {
            settings::FixMode::Diff => {
                let diff = Diff::new(member.contents(), &buffer);
                let mut stderr = workspace.error_stream();

                stream_writeln!(stderr, "{}", diff);
            }
            settings::FixMode::Generate => {}
            settings::FixMode::Apply => {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&member.path)
                    .expect("failed to open file");

                file.write_all(buffer.as_bytes()).expect("failed to write to file");
            }
        }
    }

    Ok(pipeline_diagnostics)
}
