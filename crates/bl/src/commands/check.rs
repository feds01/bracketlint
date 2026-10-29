//! Implementation of the `check` command.

use std::path::PathBuf;

use anyhow::Result;
use bl_arena::Arena;
use bl_ast::SourceId;
use bl_parse::{ParseQuery, ParseQueryResult, format_source_tree, parse_source};
use bl_reporting::Reports;
use bl_utils::{
    pool::{self, map_in_parallel},
    timed,
};
use bl_workspace::{
    Workspace, WorkspaceMembers, resolver::find_files_in_paths, settings::Settings,
};
use log::{Level, debug, info, warn};

use super::{read_members, sort_files};

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
    // which each take the next file in turn.
    let threads = pool::threads();

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.
    let diagnostics = timed(
        || {
            let members = read_members(files, workspace, threads);

            // Each thread parses each file into its own arena, and then frees it,
            // so that its next file reuses its memory.
            let (all, settings) = (&workspace.members, &workspace.settings);
            let checked = map_in_parallel(&members, threads, Arena::new, |arena, member| {
                let id = *member.as_ref().ok()?;
                let checked = check_member(all, settings, id, arena);
                arena.reset();
                Some(checked)
            });

            // The trees are dumped, and the diagnostics of each file reported, in
            // the order of the files.
            let mut diagnostics = Reports::default();
            for (member, checked) in members.into_iter().zip(checked) {
                match (member, checked) {
                    (Ok(_), Some(checked)) => {
                        if let Some(tree) = checked.tree {
                            info!("{tree}");
                        }
                        diagnostics.extend(checked.diagnostics);
                    }
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

/// What came of parsing one template.
struct CheckedMember {
    /// The diagnostics from parsing the template.
    diagnostics: Reports,

    /// The tree of the template, drawn for `--dump-ast` if the settings ask
    /// for it, since the tree is freed with its arena before it is dumped.
    tree: Option<String>,
}

/// Parse the member `id` into `arena`, and draw its tree if the settings ask
/// for it to be dumped.
fn check_member(
    members: &WorkspaceMembers,
    settings: &Settings,
    id: SourceId,
    arena: &Arena,
) -> CheckedMember {
    let member = members.member(id);
    let ParseQueryResult { node, diagnostics } = parse_source(ParseQuery::new(id, member), arena);

    let tree = node
        .filter(|_| settings.should_dump_ast())
        .map(|document| format_source_tree(member, document));

    CheckedMember { diagnostics, tree }
}
