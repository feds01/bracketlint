//! The report of how the corpus does, which is recorded in `report.md`.

use std::{cmp::Reverse, collections::BTreeMap, fmt::Write};

use super::check::{CheckedSource, Outcome, Status};

/// The statuses that the report counts besides passing, with their headings.
const FAILURES: [(Status, &str); 4] = [
    (Status::ParseError, "Parse errors"),
    (Status::FmtError, "Format errors"),
    (Status::ChangedTags, "Changed tags"),
    (Status::Unstable, "Unstable"),
];

/// How many of the most common errors in each dialect the report lists.
const COMMON_ERRORS: usize = 10;

/// A report of how many templates of each dialect and of each source pass, and
/// the most common errors in each dialect.
pub fn render(results: &[CheckedSource]) -> String {
    let mut by_dialect: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
    for checked in results {
        let outcomes = by_dialect.entry(checked.source.dialect.name()).or_default();
        outcomes.extend(checked.outcomes.values());
    }

    let mut text = String::from(
        "# Corpus report\n\nHow bracketlint does on the real-world templates of each source in \
         `sources.toml`. A template passes if it parses, formats without errors, keeps its tags \
         and formats the same way twice. `just update-snapshots` records this report again.\n\n",
    );

    text += &header(&["Dialect"]);
    for (dialect, outcomes) in &by_dialect {
        writeln!(text, "| {dialect} | {} |", row(outcomes.iter().copied())).unwrap();
    }

    text += "\n";
    text += &header(&["Source", "Dialect"]);
    for CheckedSource { source, outcomes } in results {
        writeln!(text, "| {} | {} | {} |", source.name, source.dialect, row(outcomes.values()))
            .unwrap();
    }

    text += "\n## Most common errors\n";
    for (dialect, outcomes) in &by_dialect {
        write!(text, "\n### {dialect}\n\n| Templates | First error |\n|--:|---|\n").unwrap();
        for (error, count) in common_errors(outcomes) {
            writeln!(text, "| {count} | {} |", error.replace('|', "\\|")).unwrap();
        }
    }

    text
}

/// The header of a table whose rows start with the cells `names`, followed by
/// the cells of a [row].
fn header(names: &[&str]) -> String {
    let counts = ["Templates", "Passing"].into_iter().chain(FAILURES.map(|(_, heading)| heading));
    let headings: Vec<_> = names.iter().copied().chain(counts).collect();

    let alignments: Vec<_> =
        (0..headings.len()).map(|i| if i < names.len() { "---" } else { "--:" }).collect();

    format!("| {} |\n|{}|\n", headings.join(" | "), alignments.join("|"))
}

/// The cells of a table row for `outcomes`: the number of templates, how many
/// pass, and how many have each of the [FAILURES].
fn row<'a>(outcomes: impl IntoIterator<Item = &'a Outcome>) -> String {
    let mut counts: BTreeMap<Status, usize> = BTreeMap::new();
    for outcome in outcomes {
        *counts.entry(outcome.status).or_default() += 1;
    }

    let count = |status| counts.get(&status).copied().unwrap_or(0);
    let (total, ok) = (counts.values().sum::<usize>(), count(Status::Ok));

    let mut row = format!("{total} | {ok} ({}%)", (ok * 100).checked_div(total).unwrap_or(0));
    for (status, _) in FAILURES {
        write!(row, " | {}", count(status)).unwrap();
    }
    row
}

/// The [COMMON_ERRORS] most common first errors in `outcomes`, with how many
/// templates have each.
fn common_errors<'a>(outcomes: &[&'a Outcome]) -> Vec<(&'a str, usize)> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for error in outcomes.iter().filter_map(|outcome| outcome.error.as_deref()) {
        *counts.entry(error).or_default() += 1;
    }

    // The sort is stable, so errors that are as common stay in order.
    let mut errors: Vec<_> = counts.into_iter().collect();
    errors.sort_by_key(|&(_, count)| Reverse(count));
    errors.truncate(COMMON_ERRORS);
    errors
}
