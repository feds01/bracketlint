//! The comparison of how the corpus does with its baselines on another branch,
//! which CI comments on each pull request with, e.g. which templates it fixes.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    fs, io,
    path::Path,
};

use tabled::Tabled;

use super::{
    check::{CheckedSource, Outcome, Status, parse_baseline},
    report::{cell, passing, table},
    source::Source,
};

/// How a template does compared with the base, in the order of the comment.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Change {
    /// It has a worse [Status] than in the base.
    Regressed,

    /// It has a better [Status] than in the base.
    Improved,

    /// The base doesn't have it, e.g. since its source was added.
    Added,

    /// Only the base has it.
    Removed,

    /// It has the same [Status] as in the base.
    Untouched,
}

impl Change {
    const ALL: [Change; 5] =
        [Change::Regressed, Change::Improved, Change::Added, Change::Removed, Change::Untouched];

    /// What the comment calls the templates with the change.
    fn name(self) -> &'static str {
        match self {
            Change::Regressed => "regressed",
            Change::Improved => "improved",
            Change::Added => "new",
            Change::Removed => "removed",
            Change::Untouched => "untouched",
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            Change::Regressed => "❌",
            Change::Improved => "⚡",
            Change::Added => "🆕",
            Change::Removed => "🗑️",
            Change::Untouched => "✅",
        }
    }
}

/// A template of a source, with how it does now and its status in the base.
struct Template<'a> {
    source: &'a Source,
    path: String,

    /// Its status in the base, unless the base doesn't have it.
    before: Option<Status>,

    /// How it does now, unless only the base has it.
    after: Option<&'a Outcome>,
}

impl Template<'_> {
    fn change(&self) -> Change {
        match (self.before, self.after) {
            (None, _) => Change::Added,
            (_, None) => Change::Removed,
            (Some(before), Some(after)) => match after.status.cmp(&before) {
                Ordering::Less => Change::Regressed,
                Ordering::Greater => Change::Improved,
                Ordering::Equal => Change::Untouched,
            },
        }
    }
}

/// How many templates passed in the base, and how many pass now.
#[derive(Tabled)]
struct Passing {
    #[tabled(rename = "Templates")]
    templates: usize,
    #[tabled(rename = "Passing before")]
    before: String,
    #[tabled(rename = "Passing after")]
    after: String,
    #[tabled(rename = "Change")]
    change: String,
}

impl Passing {
    fn new<'a>(templates: impl IntoIterator<Item = &'a Template<'a>>) -> Self {
        let templates: Vec<_> = templates.into_iter().collect();
        let before: Vec<_> = templates.iter().filter_map(|template| template.before).collect();
        let after: Vec<_> =
            templates.iter().filter_map(|template| Some(template.after?.status)).collect();

        let ok = |statuses: &[Status]| statuses.iter().filter(|&&s| s == Status::Ok).count();
        let (ok_before, ok_after) = (ok(&before), ok(&after));

        Passing {
            templates: after.len(),
            before: passing(ok_before, before.len()),
            after: passing(ok_after, after.len()),
            change: match ok_after as isize - ok_before as isize {
                0 => String::new(),
                change => format!("{change:+}"),
            },
        }
    }
}

#[derive(Tabled)]
struct DialectRow<'a> {
    #[tabled(rename = "Dialect")]
    dialect: &'a str,
    #[tabled(inline)]
    passing: Passing,
}

#[derive(Tabled)]
struct SourceRow<'a> {
    #[tabled(rename = "Source")]
    source: &'a str,
    #[tabled(rename = "Dialect")]
    dialect: &'a str,
    #[tabled(inline)]
    passing: Passing,
}

#[derive(Tabled)]
struct TemplateRow<'a> {
    #[tabled(rename = "Source")]
    source: &'a str,
    #[tabled(rename = "Template")]
    path: String,
    #[tabled(rename = "Before")]
    before: &'static str,
    #[tabled(rename = "After")]
    after: &'static str,
    #[tabled(rename = "First error")]
    error: String,
}

/// A comment on how the corpus does compared with the baselines in `base`, of
/// the branch that a pull request merges into: how many templates of each
/// dialect pass, and which templates do better or worse.
pub fn render(results: &[CheckedSource], base: &Path) -> String {
    let templates = templates(results, base);
    let with = |change| -> Vec<_> {
        templates.iter().filter(|template| template.change() == change).collect()
    };
    let (improved, regressed) = (with(Change::Improved).len(), with(Change::Regressed).len());

    let headline = match (improved, regressed) {
        (0, 0) => "**not change the corpus**".to_string(),
        (_, 0) => format!("**improve {improved} {}**", plural(improved)),
        (0, _) => format!("**regress {regressed} {}**", plural(regressed)),
        _ => format!("**improve {improved} {}** and **regress {regressed}**", plural(improved)),
    };

    let tallies: Vec<_> = Change::ALL
        .into_iter()
        .map(|change| (change, with(change).len()))
        .filter(|&(_, count)| count > 0)
        .map(|(change, count)| format!("`{} {count}` {}", change.emoji(), change.name()))
        .collect();

    let mut by_dialect: BTreeMap<&str, Vec<&Template>> = BTreeMap::new();
    for template in &templates {
        by_dialect.entry(template.source.dialect.name()).or_default().push(template);
    }

    let mut dialects: Vec<_> = by_dialect
        .into_iter()
        .map(|(dialect, templates)| DialectRow { dialect, passing: Passing::new(templates) })
        .collect();
    dialects.push(DialectRow { dialect: "**All**", passing: Passing::new(&templates) });

    let mut text = format!(
        "## Merging this PR will {headline}\n\n{}\n\n{}\n\n",
        tallies.join(" · "),
        table(dialects)
    );

    // The regressed templates are open, since they need looking at.
    for change in Change::ALL.into_iter().filter(|&change| change != Change::Untouched) {
        let changed = with(change);
        if changed.is_empty() {
            continue;
        }

        let count = changed.len();
        let open = if change == Change::Regressed { " open" } else { "" };
        let rows = changed.into_iter().map(|template| TemplateRow {
            source: &template.source.name,
            path: format!("`{}`", template.path),
            before: template.before.map_or("", Status::name),
            after: template.after.map_or("", |outcome| outcome.status.name()),
            error: template
                .after
                .and_then(|outcome| outcome.error.as_deref())
                .map_or_else(String::new, cell),
        });

        write!(
            text,
            "<details{open}><summary>{count} {} {}</summary>\n\n{}\n\n</details>\n\n",
            change.name(),
            plural(count),
            table(rows)
        )
        .unwrap();
    }

    // The templates are in the order of their sources, so each source's are
    // together.
    let sources = templates.chunk_by(|a, b| a.source.name == b.source.name).map(|templates| {
        let source = templates[0].source;
        SourceRow {
            source: &source.name,
            dialect: source.dialect.name(),
            passing: Passing::new(templates),
        }
    });
    write!(text, "<details><summary>Sources</summary>\n\n{}\n\n</details>\n", table(sources))
        .unwrap();

    text
}

/// Every template of each source in `results`, now or in its baseline in
/// `base`, by source and then by path.
fn templates<'a>(results: &'a [CheckedSource], base: &Path) -> Vec<Template<'a>> {
    let mut templates = vec![];

    for CheckedSource { source, outcomes } in results {
        let before = read_baseline(base, &source.name);
        let paths: BTreeSet<_> = before.keys().chain(outcomes.keys()).collect();

        templates.extend(paths.into_iter().map(|path| Template {
            source,
            path: path.clone(),
            before: before.get(path).copied(),
            after: outcomes.get(path),
        }));
    }

    templates
}

/// The status of each template of `source` in its baseline in `dir`, by path.
/// A source without a baseline there, such as one that was added since, has
/// no templates.
fn read_baseline(dir: &Path, source: &str) -> BTreeMap<String, Status> {
    let path = dir.join(format!("{source}.txt"));
    match fs::read_to_string(&path) {
        Ok(baseline) => parse_baseline(&baseline),
        Err(err) if err.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
        Err(err) => panic!("couldn't read `{}`: {err}", path.display()),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "template" } else { "templates" }
}
