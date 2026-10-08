//! The report of how the corpus does, which the test prints.

use std::{collections::BTreeMap, fmt::Write};

use bl_ast::Dialect;
use tabled::{Table, Tabled, settings::Style};

use super::check::{CheckedSource, Outcome, Status};

/// How many of the most common errors in each dialect the report lists.
const COMMON_ERRORS: usize = 10;

/// How many templates there are, how many pass, and how many have each other
/// [Status].
#[derive(Tabled)]
struct Counts {
    #[tabled(rename = "Templates")]
    templates: usize,
    #[tabled(rename = "Passing")]
    passing: String,
    #[tabled(rename = "Parse errors")]
    parse_errors: usize,
    #[tabled(rename = "Format errors")]
    fmt_errors: usize,
    #[tabled(rename = "Changed tags")]
    changed_tags: usize,
    #[tabled(rename = "Unstable")]
    unstable: usize,
}

impl Counts {
    fn new<'a>(outcomes: impl IntoIterator<Item = &'a Outcome>) -> Self {
        let statuses: Vec<_> = outcomes.into_iter().map(|outcome| outcome.status).collect();
        let count = |status| statuses.iter().filter(|&&other| other == status).count();
        let (templates, ok) = (statuses.len(), count(Status::Ok));

        Counts {
            templates,
            passing: format!("{ok} ({}%)", (ok * 100).checked_div(templates).unwrap_or(0)),
            parse_errors: count(Status::ParseError),
            fmt_errors: count(Status::FmtError),
            changed_tags: count(Status::ChangedTags),
            unstable: count(Status::Unstable),
        }
    }
}

#[derive(Tabled)]
struct DialectRow<'a> {
    #[tabled(rename = "Dialect")]
    dialect: &'a str,
    #[tabled(inline)]
    counts: Counts,
}

#[derive(Tabled)]
struct SourceRow<'a> {
    #[tabled(rename = "Source")]
    source: &'a str,
    #[tabled(rename = "Dialect")]
    dialect: Dialect,
    #[tabled(inline)]
    counts: Counts,
}

#[derive(Tabled)]
struct ErrorRow {
    #[tabled(rename = "Templates")]
    templates: usize,
    #[tabled(rename = "First error")]
    error: String,
}

/// A report of how many templates of each dialect and of each source pass, and
/// the most common errors in each dialect.
pub fn render(results: &[CheckedSource]) -> String {
    let mut by_dialect: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
    for checked in results {
        let outcomes = by_dialect.entry(checked.source.dialect.name()).or_default();
        outcomes.extend(checked.outcomes.values());
    }

    let dialects = by_dialect.iter().map(|(dialect, outcomes)| DialectRow {
        dialect,
        counts: Counts::new(outcomes.iter().copied()),
    });

    let sources = results.iter().map(|CheckedSource { source, outcomes }| SourceRow {
        source: &source.name,
        dialect: source.dialect,
        counts: Counts::new(outcomes.values()),
    });

    let mut text = format!(
        "# Corpus report\n\nA template passes if it parses, formats without errors, keeps its \
         tags and formats the same way twice.\n\n{}\n\n{}\n\n## Most common errors\n",
        table(dialects),
        table(sources),
    );

    for (dialect, outcomes) in &by_dialect {
        write!(text, "\n### {dialect}\n\n{}\n", table(common_errors(outcomes))).unwrap();
    }

    text
}

fn table<T: Tabled>(rows: impl IntoIterator<Item = T>) -> String {
    Table::new(rows).with(Style::markdown()).to_string()
}

/// The [COMMON_ERRORS] most common first errors in `outcomes`, with how many
/// templates have each.
fn common_errors(outcomes: &[&Outcome]) -> Vec<ErrorRow> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for error in outcomes.iter().filter_map(|outcome| outcome.error.as_deref()) {
        *counts.entry(error).or_default() += 1;
    }

    // The sort is stable, so errors that are as common stay in order.
    let mut errors: Vec<_> = counts.into_iter().collect();
    errors.sort_by_key(|&(_, templates)| std::cmp::Reverse(templates));

    errors
        .into_iter()
        .take(COMMON_ERRORS)
        .map(|(error, templates)| ErrorRow { templates, error: error.replace('|', "\\|") })
        .collect()
}
