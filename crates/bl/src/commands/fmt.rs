//! Implementation of the `check` command.

use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use bl_ast::Arena;
use bl_fmt::{FormatQuery, FormatQueryResult, FormatterOptions, fmt_module};
use bl_lints::{diff::Diff, settings};
use bl_parse::{ParseQuery, ParseQueryResult, parse_source};
use bl_reporting::Reports;
use bl_utils::{stream_writeln, timed};
use bl_workspace::{Member, Workspace, resolver::find_files_in_paths};
use log::{Level, debug};

pub fn fmt(files: &[PathBuf], workspace: &mut Workspace) -> Result<Reports> {
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

    let options = FormatterOptions::default();
    let mut parse_diagnostics = Reports::default();
    let mut format_diagnostics = Reports::default();

    // Each file is parsed into the arena, formatted, and then freed from it, so
    // that the next file reuses its memory.
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

                        let source = workspace.reserve_member(file.into_path(), contents);
                        let member = workspace.members.member(source);

                        let ParseQueryResult { node, diagnostics } =
                            parse_source(ParseQuery::new(source, member), &arena);

                        // Files that failed to parse aren't formatted: the formatter
                        // would drop the parts that it couldn't parse.
                        let has_errors = diagnostics.iter().any(|report| report.is_error());
                        parse_diagnostics.extend(diagnostics);

                        if let Some(document) = node
                            && !has_errors
                        {
                            let FormatQueryResult { buffer, diagnostics } =
                                fmt_module(FormatQuery { member, source, options, document });
                            format_diagnostics.extend(diagnostics);
                            emit_formatted(workspace, member, &buffer);
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
            debug!("parsed and formatted files in {duration:?}");
        },
    );

    // The diagnostics from parsing each file come before those from formatting
    // them.
    parse_diagnostics.extend(format_diagnostics);
    Ok(parse_diagnostics)
}

/// Emit the formatted `buffer` of `member` as the fix mode asks: as a diff, as
/// the whole of it, or by writing it to the member's file.
///
/// @@Todo: factor this out into a general interface for emitting
/// changes/applying them.
fn emit_formatted(workspace: &Workspace, member: &Member, buffer: &str) {
    match workspace.settings.linter_settings.fix_mode {
        settings::FixMode::Diff => {
            let diff = Diff::new(member.contents(), buffer);
            let mut stderr = workspace.error_stream();

            stream_writeln!(stderr, "{}", diff);
        }
        settings::FixMode::Generate => {
            let mut stdout = workspace.output_stream();
            stream_writeln!(stdout, "{}", buffer);
        }
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
