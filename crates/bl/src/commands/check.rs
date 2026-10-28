//! Implementation of the `check` command.

use std::{fs, num::NonZero, path::PathBuf, thread};

use anyhow::Result;
use bl_arena::Arena;
use bl_ast::SourceId;
use bl_parse::{ParseQuery, ParseQueryResult, emit_source_tree, parse_source};
use bl_reporting::Reports;
use bl_utils::timed;
use bl_workspace::{
    Workspace, WorkspaceMembers, resolver::find_files_in_paths, settings::Settings,
};
use log::{Level, debug, warn};

use super::{FileError, map_in_parallel, sort_files};

pub fn check(files: &[PathBuf], workspace: &mut Workspace) -> Result<Reports> {
    // Firstly, we need to discover all of the files in the provided paths.
    let mut files = timed(
        || find_files_in_paths(files, &workspace.settings),
        log::Level::Debug,
        |duration| debug!("resolved files in {duration:?}"),
    )?;
    sort_files(&mut files);

    if files.is_empty() {
        warn!("no template files found in the given paths");
        return Ok(Reports::default());
    }

    debug!("found {} files", files.len());

    // The files are read and parsed on as many threads as the machine has,
    // which each take the next file in turn. A file's tree is dumped as it is
    // parsed, so `--dump-ast` takes one thread, to dump the trees in order.
    let threads = if workspace.settings.should_dump_ast() {
        1
    } else {
        thread::available_parallelism().map_or(1, NonZero::get)
    };

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.
    let diagnostics = timed(
        || {
            let files: Vec<_> =
                files.into_iter().map(|file| file.map(|file| file.into_path())).collect();

            // Reading the files takes longer than parsing them, so they are read
            // in parallel too, before their members are reserved in order.
            let contents = map_in_parallel(
                &files,
                threads,
                || (),
                |(), file| file.as_ref().ok().map(fs::read_to_string),
            );

            let checks: Vec<_> = files
                .into_iter()
                .zip(contents)
                .map(|(file, contents)| match (file, contents) {
                    (Ok(path), Some(Ok(contents))) => Ok(workspace.reserve_member(path, contents)),
                    (Ok(path), Some(Err(err))) => Err(Reports::from(FileError::Read { path, err })),
                    (Err(err), _) => Err(Reports::from(FileError::from(err))),
                    (Ok(_), None) => unreachable!("each path that was found is read"),
                })
                .collect();

            // Each thread parses each file into its own arena, and then frees it,
            // so that its next file reuses its memory.
            let (members, settings) = (&workspace.members, &workspace.settings);
            let mut parsed = map_in_parallel(&checks, threads, Arena::new, |arena, check| {
                let id = *check.as_ref().ok()?;
                let diagnostics = check_member(members, settings, id, arena);
                arena.reset();
                Some(diagnostics)
            })
            .into_iter();

            // The diagnostics of each file are reported in the order of the files.
            let mut diagnostics = Reports::default();
            for check in checks {
                match (check, parsed.next().flatten()) {
                    (Ok(_), Some(parsed)) => diagnostics.extend(parsed),
                    (Err(reports), _) => diagnostics.extend(reports),
                    (Ok(_), None) => unreachable!("each member is parsed"),
                }
            }

            diagnostics
        },
        Level::Debug,
        |duration| {
            debug!("read and parsed files in {duration:?}");
        },
    );

    Ok(diagnostics)
}

/// Parse the member `id` into `arena`, dump its tree if the settings ask for
/// it, and return the diagnostics from parsing it.
fn check_member(
    members: &WorkspaceMembers,
    settings: &Settings,
    id: SourceId,
    arena: &Arena,
) -> Reports {
    let member = members.member(id);
    let ParseQueryResult { node, diagnostics } = parse_source(ParseQuery::new(id, member), arena);

    if let Some(document) = node
        && settings.should_dump_ast()
    {
        emit_source_tree(member, document);
    }

    diagnostics
}
