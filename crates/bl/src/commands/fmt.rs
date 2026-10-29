//! Implementation of the `fmt` command.

use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use bl_arena::Arena;
use bl_ast::SourceId;
use bl_fmt::{FormatQuery, FormatQueryResult, FormatterOptions, fmt_module};
use bl_lints::{diff::Diff, settings::FixMode};
use bl_parse::{ParseQuery, ParseQueryResult, parse_source};
use bl_reporting::{Reports, pluralise};
use bl_utils::{
    path::relative_path,
    pool::{self, map_in_parallel},
    stream_write, stream_writeln, timed,
};
use bl_workspace::{Member, Workspace, resolver::find_files_in_paths};
use log::{Level, debug, warn};

use super::{FileError, read_members, sort_files};

/// What came of formatting the templates.
#[derive(Debug, Default)]
pub struct FormatResults {
    /// The diagnostics from parsing the templates, and then from formatting
    /// them.
    pub reports: Reports,

    /// How many of the templates formatting changes.
    pub counts: FormatCounts,
}

/// How many templates formatting changes, and how many are already formatted.
/// A template that fails to parse isn't formatted, so it is in neither.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FormatCounts {
    /// The templates that formatting changes, which are written back unless
    /// the fix mode only shows them.
    pub changed: usize,

    /// The templates that are already formatted.
    pub unchanged: usize,
}

impl FormatCounts {
    /// A summary of the counts in the words of `fix_mode`, as `ruff format`
    /// gives, e.g. `2 files reformatted, 1 file left unchanged`, or `None` if
    /// no template was formatted.
    pub fn summary(self, fix_mode: FixMode) -> Option<String> {
        let (changed, unchanged) = match fix_mode {
            FixMode::Check | FixMode::Diff => ("would be reformatted", "already formatted"),
            FixMode::Apply | FixMode::Generate => ("reformatted", "left unchanged"),
        };

        let summary = [(self.changed, changed), (self.unchanged, unchanged)]
            .into_iter()
            .filter(|&(count, _)| count != 0)
            .map(|(count, outcome)| format!("{count} file{} {outcome}", pluralise!(count)))
            .collect::<Vec<_>>()
            .join(", ");

        (!summary.is_empty()).then_some(summary)
    }
}

pub fn fmt(files: &[PathBuf], workspace: &mut Workspace) -> Result<FormatResults> {
    // Firstly, we need to discover all of the files in the provided paths.
    let mut files = timed(
        || find_files_in_paths(files, &workspace.settings),
        log::Level::Debug,
        |duration| debug!("resolved files in {duration:?}"),
    )?;
    sort_files(&mut files);

    if files.is_empty() {
        warn!("no template files found in the given paths");
        return Ok(FormatResults::default());
    }

    debug!("found {} files", files.len());

    let options =
        FormatterOptions { markup: workspace.settings.markup, ..FormatterOptions::default() };
    let fix_mode = workspace.settings.linter_settings.fix_mode;
    let mut counts = FormatCounts::default();
    let mut parse_diagnostics = Reports::default();
    let mut format_diagnostics = Reports::default();

    // The files are read, parsed and formatted on as many threads as the
    // machine has, which each take the next file in turn.
    let threads = pool::threads();

    // @@Todo: integrate a cache system here, we should be able to avoid re-linting
    // already existent files and just skip them.
    timed(
        || {
            let members = read_members(files, workspace, threads);

            // Each thread parses each file into its own arena, formats it, and
            // then frees it, so that its next file reuses its memory.
            let all = &workspace.members;
            let formatted = map_in_parallel(&members, threads, Arena::new, |arena, member| {
                let id = *member.as_ref().ok()?;
                let formatted = format_member(all.member(id), id, arena, options, fix_mode);
                arena.reset();
                Some(formatted)
            });

            // Each file is emitted, and its diagnostics reported, in the order of
            // the files, since emitting it can write to standard output.
            for (member, formatted) in members.into_iter().zip(formatted) {
                let (id, formatted) = match (member, formatted) {
                    (Ok(id), Some(formatted)) => (id, formatted),
                    (Err(reports), _) => {
                        parse_diagnostics.extend(reports);
                        continue;
                    }
                    (Ok(_), None) => unreachable!("each member is formatted"),
                };

                let FormattedMember {
                    parse_diagnostics: parsed,
                    format_diagnostics: mut formats,
                    changed,
                    buffer,
                } = formatted;
                parse_diagnostics.extend(parsed);

                if let Some(changed) = changed {
                    let member = workspace.members.member(id);
                    let emitted = match &buffer {
                        Some(buffer) => emit_formatted(workspace, member, buffer, changed),
                        None => Ok(()),
                    };

                    match emitted {
                        Ok(()) if changed => counts.changed += 1,
                        Ok(()) => counts.unchanged += 1,
                        Err(err) => formats.extend(Reports::from(err)),
                    }
                }

                format_diagnostics.extend(formats);
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
    Ok(FormatResults { reports: parse_diagnostics, counts })
}

/// What came of formatting one template.
#[derive(Default)]
struct FormattedMember {
    /// The diagnostics from parsing the template.
    parse_diagnostics: Reports,

    /// The diagnostics from formatting the template.
    format_diagnostics: Reports,

    /// Whether formatting changes the template, or `None` if it isn't
    /// formatted.
    changed: Option<bool>,

    /// The formatted template, if it is to be emitted: when formatting changes
    /// it, or when the fix mode emits each template.
    buffer: Option<String>,
}

/// Parse `member`, whose ID is `source`, into `arena`, and format it if it
/// parsed without errors.
fn format_member(
    member: &Member,
    source: SourceId,
    arena: &Arena,
    options: FormatterOptions,
    fix_mode: FixMode,
) -> FormattedMember {
    let ParseQueryResult { node, diagnostics } =
        parse_source(ParseQuery::new(source, member), arena);

    // Files that failed to parse aren't formatted: the formatter would drop the
    // parts that it couldn't parse.
    let has_errors = diagnostics.iter().any(|report| report.is_error());
    let Some(document) = node.filter(|_| !has_errors) else {
        return FormattedMember { parse_diagnostics: diagnostics, ..Default::default() };
    };

    let FormatQueryResult { buffer, diagnostics: format_diagnostics } =
        fmt_module(FormatQuery { member, source, options, document });
    let changed = buffer != member.contents();
    let buffer = (changed || matches!(fix_mode, FixMode::Generate)).then_some(buffer);

    FormattedMember {
        parse_diagnostics: diagnostics,
        format_diagnostics,
        changed: Some(changed),
        buffer,
    }
}

/// Emit the formatted `buffer` of `member` as the fix mode asks: as the whole
/// of it, or, if formatting `changed` the member, by writing it to the
/// member's file, naming the file, or as a diff. Fails with an error if the
/// file couldn't be written.
///
/// @@Todo: factor this out into a general interface for emitting
/// changes/applying them.
fn emit_formatted(
    workspace: &Workspace,
    member: &Member,
    buffer: &str,
    changed: bool,
) -> Result<(), FileError> {
    let mut stdout = workspace.output_stream();

    match workspace.settings.linter_settings.fix_mode {
        FixMode::Generate => stream_writeln!(stdout, "{}", buffer),

        // A file that is already formatted is left as it is, and isn't shown.
        _ if !changed => {}

        FixMode::Apply => {
            fs::write(&member.path, buffer)
                .map_err(|err| FileError::Write { path: member.path.clone(), err })?;
        }
        FixMode::Check => {
            stream_writeln!(stdout, "Would reformat: {}", relative_path(&member.path).display());
        }
        FixMode::Diff => {
            let path = relative_path(&member.path).display().to_string();
            let diff = Diff::new(member.contents(), buffer).with_header(&path, &path);
            stream_write!(stdout, "{diff}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_summary() {
        let counts = FormatCounts { changed: 2, unchanged: 1 };
        assert_eq!(
            counts.summary(FixMode::Apply).as_deref(),
            Some("2 files reformatted, 1 file left unchanged")
        );
        assert_eq!(
            counts.summary(FixMode::Check).as_deref(),
            Some("2 files would be reformatted, 1 file already formatted")
        );

        let counts = FormatCounts { changed: 0, unchanged: 3 };
        assert_eq!(counts.summary(FixMode::Diff).as_deref(), Some("3 files already formatted"));

        assert_eq!(FormatCounts::default().summary(FixMode::Apply), None);
    }
}
