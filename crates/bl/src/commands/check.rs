//! Implementation of the `check` command.

use std::{fs, path::PathBuf};

use anyhow::Result;
use bl_ast::Arena;
use bl_parse::{ParseQuery, ParseQueryResult, emit_source_tree, parse_source};
use bl_reporting::Reports;
use bl_utils::timed;
use bl_workspace::{Workspace, resolver::find_files_in_paths};
use log::{Level, debug};

pub fn check(files: &[PathBuf], workspace: &mut Workspace) -> Result<Reports> {
    // Firstly, we need to discover all of the files in the provided paths.
    let files = timed(
        || find_files_in_paths(files, &workspace.settings),
        log::Level::Debug,
        |duration| debug!("resolved files in {duration:?}"),
    )?;

    if files.is_empty() {
        // @@Todo: warn the user that there were no files to check.
        return Ok(Reports::default());
    }

    debug!("found {} files", files.len());

    let mut pipeline_diagnostics = Reports::default();

    // Each file is parsed into the arena, and then freed from it, so that the
    // next file reuses its memory.
    let mut arena = Arena::new();

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

                        let id = workspace.reserve_member(file.into_path(), contents);
                        let member = workspace.members.member(id);

                        let ParseQueryResult { node, diagnostics } =
                            parse_source(ParseQuery::new(id, member), &arena);

                        pipeline_diagnostics.extend(diagnostics);

                        if let Some(document) = node
                            && workspace.settings.should_dump_ast()
                        {
                            emit_source_tree(member, document);
                        }

                        arena.reset();
                    }
                    Err(err) => {
                        // @@todo: Create a diagnostic for the error, and print it!
                        println!("error: {}", err);
                    }
                }
            }
        },
        Level::Debug,
        |duration| {
            debug!("parsed files in {duration:?}");
        },
    );

    Ok(pipeline_diagnostics)
}
