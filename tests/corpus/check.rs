//! Checking the templates of the corpus by parsing and formatting them, as
//! `bracketlint check` and `bracketlint fmt` do.

use std::{
    collections::BTreeMap,
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
};

use bl_ast::{Arena, Dialect};
use bl_fmt::{FormatQuery, FormatQueryResult, FormatterOptions, fmt_module};
use bl_parse::{ParseQuery, ParseQueryResult, parse_source};
use bl_reporting::Reports;
use bl_workspace::WorkspaceMembers;
use strum::{EnumString, IntoStaticStr};

use super::source::Source;

/// How a template does, from worst to best. A template gets the first status
/// that applies to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Status {
    /// Parsing it reports an error.
    ParseError,

    /// Formatting it reports an error.
    FmtError,

    /// Formatting it changed a `{% %}`, `{{ }}` or `{# #}` tag other than by
    /// its whitespace, i.e. it dropped or changed template source.
    ChangedTags,

    /// Formatting it again changes it again, or fails.
    Unstable,

    /// None of the above.
    Ok,
}

impl Status {
    /// The name of the status in the baselines, e.g. `parse-error`.
    pub fn name(self) -> &'static str {
        self.into()
    }
}

/// How a template does, with the first error that it reported.
pub struct Outcome {
    pub status: Status,
    pub error: Option<String>,
}

impl Outcome {
    const OK: Outcome = Outcome { status: Status::Ok, error: None };

    fn failed(status: Status, error: impl Into<String>) -> Self {
        Outcome { status, error: Some(error.into()) }
    }
}

/// A [Source], with how each of its templates does by path.
pub struct CheckedSource<'a> {
    pub source: &'a Source,
    pub outcomes: BTreeMap<String, Outcome>,
}

impl<'a> CheckedSource<'a> {
    /// Check every template of `source`.
    pub fn new(source: &'a Source) -> Self {
        let outcomes = source
            .templates()
            .into_iter()
            .map(|template| (template.path, check(source.dialect, &template.contents)))
            .collect();

        CheckedSource { source, outcomes }
    }

    /// The baseline of the source: the status of each template, a line each.
    pub fn baseline(&self) -> String {
        let line = |(path, outcome): (&String, &Outcome)| {
            format!("{:<13} {path}\n", outcome.status.name())
        };
        self.outcomes.iter().map(line).collect()
    }
}

/// The status of each template in a `baseline`, by path, see
/// [CheckedSource::baseline].
pub fn parse_baseline(baseline: &str) -> BTreeMap<String, Status> {
    let template = |line: &str| {
        let (status, path) = line.split_once(' ').expect("a status and a path");
        let status = status.parse().unwrap_or_else(|_| panic!("unknown status `{status}`"));
        (path.trim_start().to_string(), status)
    };
    baseline.lines().map(template).collect()
}

/// How the template `contents` does in `dialect`.
fn check(dialect: Dialect, contents: &str) -> Outcome {
    let formatted = match format(dialect, contents) {
        Ok(formatted) => formatted,
        Err(failed) => return failed,
    };

    let (before, after) = (tags(contents), tags(&formatted));
    if before != after {
        let changed = before.iter().zip(&after).find(|(old, new)| old != new);
        let error = changed.map_or("a tag was added or removed", |(old, _)| old.as_str());
        return Outcome::failed(Status::ChangedTags, error);
    }

    match format(dialect, &formatted) {
        Ok(again) if again == formatted => Outcome::OK,
        Ok(_) => Outcome { status: Status::Unstable, error: None },
        Err(failed) => Outcome {
            status: Status::Unstable,
            error: failed.error.map(|error| format!("once formatted: {error}")),
        },
    }
}

/// Parse and format `contents` in `dialect`, as `bracketlint fmt` does.
fn format(dialect: Dialect, contents: &str) -> Result<String, Outcome> {
    let mut members = WorkspaceMembers::new();
    let id = members.reserve_member(PathBuf::from("template"), contents.to_string(), dialect);
    let member = members.member(id);
    let arena = Arena::new();

    let document = run_stage(Status::ParseError, || {
        let ParseQueryResult { node, diagnostics } =
            parse_source(ParseQuery::new(id, member), &arena);
        (node, diagnostics)
    })?;

    run_stage(Status::FmtError, || {
        let document = document.expect("Attempted to format a module without a document.");
        let options = FormatterOptions::default();
        let FormatQueryResult { buffer, diagnostics } =
            fmt_module(FormatQuery { member, source: id, options, document });
        (buffer, diagnostics)
    })
}

/// Run a stage of bracketlint, which fails with `status` if it reports an
/// error or panics.
fn run_stage<T>(status: Status, stage: impl FnOnce() -> (T, Reports)) -> Result<T, Outcome> {
    let (output, reports) = panic::catch_unwind(AssertUnwindSafe(stage)).map_err(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str));
        Outcome::failed(status, format!("panicked: {}", message.unwrap_or_default()))
    })?;

    match reports.iter().find(|report| report.is_error()) {
        Some(report) => Err(Outcome::failed(status, &report.title)),
        None => Ok(output),
    }
}

/// The template tags in `text`, i.e. its `{% %}`, `{{ }}` and `{# #}`, without
/// their whitespace.
fn tags(text: &str) -> Vec<String> {
    let mut tags = vec![];
    let mut rest = text;

    while let Some(start) = rest.find('{') {
        let end = match rest[start + 1..].chars().next() {
            Some('%') => "%}",
            Some('{') => "}}",
            Some('#') => "#}",
            _ => {
                rest = &rest[start + 1..];
                continue;
            }
        };

        let Some(length) = rest[start + 2..].find(end) else { break };
        let tag = &rest[start..start + 2 + length + 2];
        tags.push(tag.split_whitespace().collect());
        rest = &rest[start + tag.len()..];
    }

    tags
}

#[test]
fn test_tags() {
    let text = "<p class=\"{ a }\">{{ a }}{%- if b -%}{#\n c #}</p>{% unclosed";
    assert_eq!(tags(text), ["{{a}}", "{%-ifb-%}", "{#c#}"]);
}
