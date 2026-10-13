//! Implementation of the `fmt` command.

use std::{fs, io::Write, path::PathBuf};

use anyhow::Result;
use bl_arena::Arena;
use bl_fmt::{FormatQuery, FormatQueryResult, FormatterOptions, fmt_module};
use bl_lints::{diff::Diff, settings::FixMode};
use bl_parse::{ParseQuery, ParseQueryResult, parse_source};
use bl_reporting::{Reports, pluralise};
use bl_utils::{stream_write, stream_writeln, timed};
use bl_workspace::{Member, Workspace, resolver::find_files_in_paths};
use log::{Level, debug};

use super::relative_path;

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
    let files = timed(
        || find_files_in_paths(files, &workspace.settings),
        log::Level::Debug,
        |duration| debug!("resolved files in {duration:?}"),
    )?;

    if files.is_empty() {
        // @@Todo: warn the user that there were no files to check.
        return Ok(FormatResults::default());
    }

    debug!("found {} files", files.len());

    let options = FormatterOptions::default();
    let mut counts = FormatCounts::default();
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
                        let formatted = format_file(workspace, file.into_path(), &arena, options);
                        parse_diagnostics.extend(formatted.parse_diagnostics);
                        format_diagnostics.extend(formatted.format_diagnostics);

                        match formatted.changed {
                            Some(true) => counts.changed += 1,
                            Some(false) => counts.unchanged += 1,
                            None => {}
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
    Ok(FormatResults { reports: parse_diagnostics, counts })
}

/// What came of formatting one template.
#[derive(Default)]
struct FormattedFile {
    /// The diagnostics from parsing the template.
    parse_diagnostics: Reports,

    /// The diagnostics from formatting the template.
    format_diagnostics: Reports,

    /// Whether formatting changes the template, or `None` if it isn't
    /// formatted.
    changed: Option<bool>,
}

/// Parse the file at `path` into `arena`, and format it and emit the result if
/// it parsed without errors.
fn format_file(
    workspace: &mut Workspace,
    path: PathBuf,
    arena: &Arena,
    options: FormatterOptions,
) -> FormattedFile {
    // We first need to convert this to a workspace member.
    let Ok(contents) = fs::read_to_string(&path) else {
        // @@Todo: add an error for the queue.
        return FormattedFile::default();
    };

    let source = workspace.reserve_member(path, contents);
    let member = workspace.members.member(source);

    let ParseQueryResult { node, diagnostics } =
        parse_source(ParseQuery::new(source, member), arena);

    // Files that failed to parse aren't formatted: the formatter would drop the
    // parts that it couldn't parse.
    let has_errors = diagnostics.iter().any(|report| report.is_error());
    let Some(document) = node.filter(|_| !has_errors) else {
        return FormattedFile { parse_diagnostics: diagnostics, ..Default::default() };
    };

    let FormatQueryResult { buffer, diagnostics: format_diagnostics } =
        fmt_module(FormatQuery { member, source, options, document });
    let changed = buffer != member.contents();
    emit_formatted(workspace, member, &buffer, changed);

    FormattedFile { parse_diagnostics: diagnostics, format_diagnostics, changed: Some(changed) }
}

/// Emit the formatted `buffer` of `member` as the fix mode asks: as the whole
/// of it, or, if formatting `changed` the member, by writing it to the
/// member's file, naming the file, or as a diff.
///
/// @@Todo: factor this out into a general interface for emitting
/// changes/applying them.
fn emit_formatted(workspace: &Workspace, member: &Member, buffer: &str, changed: bool) {
    let mut stdout = workspace.output_stream();

    match workspace.settings.linter_settings.fix_mode {
        FixMode::Generate => stream_writeln!(stdout, "{}", buffer),

        // A file that is already formatted is left as it is, and isn't shown.
        _ if !changed => {}

        FixMode::Apply => {
            fs::write(&member.path, buffer).expect("failed to write to file");
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
