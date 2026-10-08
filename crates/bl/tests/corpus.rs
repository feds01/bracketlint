//! Checks bracketlint against real-world templates in each dialect.
//!
//! The templates come from the sources in `tests/corpus/sources.toml`, which
//! are downloaded into `target/corpus`. Each template gets the first of these
//! statuses that applies, from worst to best:
//!
//! - `parse-error`: `bracketlint check` reports an error.
//! - `fmt-error`: `bracketlint fmt` reports an error.
//! - `changed-tags`: formatting changed a `{% %}`, `{{ }}` or `{# #}` tag other
//!   than by its whitespace, i.e. it dropped or changed template source.
//! - `unstable`: formatting the formatted template changes it again.
//! - `ok`: none of the above.
//!
//! `tests/corpus/baselines` records the status of every template of each
//! source, and `tests/corpus/report.md` sums them up. Like a snapshot, the test
//! fails if either differs from how the templates do now. Run the test with
//! `REGENERATE_OUTPUT=true` (`just corpus-update`) to record them again, so
//! that support for each dialect only grows.

use std::{
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

use bl_utils::text::strip_ansi;
use serde::Deserialize;

/// A repository of templates at a pinned tag, of which only `paths` are
/// downloaded, and only files with `extension` are checked in `dialect`.
#[derive(Deserialize)]
struct Source {
    name: String,
    dialect: String,
    repo: String,
    tag: String,
    paths: Vec<String>,
    extension: String,
}

#[derive(Deserialize)]
struct Sources {
    source: Vec<Source>,
}

/// How a template does, from worst to best.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    ParseError,
    FmtError,
    ChangedTags,
    Unstable,
    Ok,
}

impl Status {
    const ALL: [Status; 5] =
        [Status::ParseError, Status::FmtError, Status::ChangedTags, Status::Unstable, Status::Ok];

    fn name(self) -> &'static str {
        match self {
            Status::ParseError => "parse-error",
            Status::FmtError => "fmt-error",
            Status::ChangedTags => "changed-tags",
            Status::Unstable => "unstable",
            Status::Ok => "ok",
        }
    }

    fn from_name(name: &str) -> Option<Status> {
        Status::ALL.into_iter().find(|status| status.name() == name)
    }
}

/// How a template does, with the first error that bracketlint reported.
struct Outcome {
    status: Status,
    error: Option<String>,
}

/// How each template of a source does, by its path.
type Outcomes = BTreeMap<String, Outcome>;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus")
}

fn baselines_dir() -> PathBuf {
    corpus_dir().join("baselines")
}

fn read_sources() -> Vec<Source> {
    let text = fs::read_to_string(corpus_dir().join("sources.toml")).unwrap();
    toml::from_str::<Sources>(&text).expect("couldn't read tests/corpus/sources.toml").source
}

/// Download the paths of `source` at its tag, unless they already are.
fn fetch(source: &Source) -> PathBuf {
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/corpus")
        .join(format!("{}@{}", source.name, source.tag));
    if target.exists() {
        return target;
    }

    let partial = target.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);

    // Git warns that an annotated tag "is not a commit" in a shallow clone, so
    // its output is only shown when it fails.
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .args(["-c", "advice.detachedHead=false"])
            .args(args)
            .output()
            .expect("couldn't run git");

        assert!(
            output.status.success(),
            "couldn't download {}:\n{}",
            source.name,
            String::from_utf8_lossy(&output.stderr)
        );
    };

    let partial_path = partial.to_str().unwrap();
    git(&[
        "clone",
        "--quiet",
        "--depth=1",
        "--filter=blob:none",
        "--sparse",
        "--branch",
        &source.tag,
        &source.repo,
        partial_path,
    ]);

    let mut sparse_checkout = vec!["-C", partial_path, "sparse-checkout", "set", "--no-cone"];
    sparse_checkout.extend(source.paths.iter().map(String::as_str));
    git(&sparse_checkout);

    fs::rename(&partial, &target).unwrap();
    target
}

/// The templates of `source` in its `checkout`, relative to it.
fn templates(source: &Source, checkout: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, extension, files);
            } else if path.extension().is_some_and(|ext| ext == extension) {
                files.push(path);
            }
        }
    }

    let mut files = vec![];
    for path in &source.paths {
        walk(&checkout.join(path), &source.extension, &mut files);
    }

    files.sort();
    files.into_iter().map(|file| file.strip_prefix(checkout).unwrap().to_path_buf()).collect()
}

/// Run bracketlint, returning the errors that it reports.
fn bracketlint(args: &[&str]) -> Vec<String> {
    let output =
        Command::new(env!("CARGO_BIN_EXE_bracketlint")).args(args).output().expect("couldn't run");

    let text = strip_ansi(&format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ));

    text.lines()
        .filter_map(|line| line.strip_prefix("error"))
        .map(|error| error.trim_start_matches(": ").to_string())
        .collect()
}

/// The template tags in `text`, without their whitespace.
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

/// How the template at `file` does in `dialect`.
fn check(dialect: &str, file: &Path) -> Outcome {
    let path = file.to_str().unwrap();
    if let Some(error) = bracketlint(&["check", "--dialect", dialect, path]).into_iter().next() {
        return Outcome { status: Status::ParseError, error: Some(error) };
    }

    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join(file.file_name().unwrap());
    fs::copy(file, &copy).unwrap();
    let copy_path = copy.to_str().unwrap();

    let format = || bracketlint(&["fmt", "--fix", "--dialect", dialect, copy_path]);
    if let Some(error) = format().into_iter().next() {
        return Outcome { status: Status::FmtError, error: Some(error) };
    }

    let source = fs::read_to_string(file).unwrap();
    let formatted = fs::read_to_string(&copy).unwrap();
    let (before, after) = (tags(&source), tags(&formatted));
    if before != after {
        let changed = before.iter().zip(&after).find(|(old, new)| old != new);
        let error =
            changed.map_or("a tag was added or removed".to_string(), |(old, _)| old.clone());
        return Outcome { status: Status::ChangedTags, error: Some(error) };
    }

    format();
    if fs::read_to_string(&copy).unwrap() != formatted {
        return Outcome { status: Status::Unstable, error: None };
    }

    Outcome { status: Status::Ok, error: None }
}

/// The baseline of `outcomes`, i.e. the status of each template.
fn baseline(outcomes: &Outcomes) -> String {
    let mut text = String::new();
    for (path, outcome) in outcomes {
        writeln!(text, "{:<13} {path}", outcome.status.name()).unwrap();
    }
    text
}

fn read_baseline(source: &str) -> BTreeMap<String, Status> {
    let path = baselines_dir().join(format!("{source}.txt"));
    let text = fs::read_to_string(path).unwrap_or_default();

    text.lines()
        .map(|line| {
            let (status, path) = line.split_once(' ').unwrap();
            (path.trim().to_string(), Status::from_name(status).unwrap())
        })
        .collect()
}

/// A row of a report table: the number of templates, how many pass, and how
/// many have each other status.
fn row(outcomes: &[&Outcome]) -> String {
    let count = |status| outcomes.iter().filter(|outcome| outcome.status == status).count();
    let (total, ok) = (outcomes.len(), count(Status::Ok));

    let mut row = format!("{total} | {ok} ({}%)", (ok * 100).checked_div(total).unwrap_or(0));
    for status in [Status::ParseError, Status::FmtError, Status::ChangedTags, Status::Unstable] {
        write!(row, " | {}", count(status)).unwrap();
    }
    row
}

/// A report of how many templates of each dialect and each source pass, and the
/// most common errors in each dialect.
fn report(sources: &[Source], results: &BTreeMap<String, Outcomes>) -> String {
    let mut by_dialect: BTreeMap<&str, Vec<&Outcome>> = BTreeMap::new();
    for source in sources {
        by_dialect.entry(&source.dialect).or_default().extend(results[&source.name].values());
    }

    let mut text = String::from(
        "# Corpus report\n\nHow bracketlint does on the real-world templates of each source in \
         `sources.toml`. A template passes if it parses, formats without errors, keeps its tags \
         and formats the same way twice. `just corpus-update` records this report again.\n\n",
    );

    let statuses = "Parse errors | Format errors | Changed tags | Unstable";
    writeln!(text, "| Dialect | Templates | Passing | {statuses} |").unwrap();
    writeln!(text, "|---|--:|--:|--:|--:|--:|--:|").unwrap();
    for (dialect, outcomes) in &by_dialect {
        writeln!(text, "| {dialect} | {} |", row(outcomes)).unwrap();
    }

    writeln!(text, "\n| Source | Dialect | Templates | Passing | {statuses} |").unwrap();
    writeln!(text, "|---|---|--:|--:|--:|--:|--:|--:|").unwrap();
    for source in sources {
        let outcomes: Vec<_> = results[&source.name].values().collect();
        writeln!(text, "| {} | {} | {} |", source.name, source.dialect, row(&outcomes)).unwrap();
    }

    write!(text, "\n## Most common errors\n").unwrap();
    for (dialect, outcomes) in &by_dialect {
        let mut errors: BTreeMap<&str, usize> = BTreeMap::new();
        for outcome in outcomes {
            if let Some(error) = &outcome.error {
                *errors.entry(error).or_default() += 1;
            }
        }

        let mut errors: Vec<_> = errors.into_iter().collect();
        errors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

        write!(text, "\n### {dialect}\n\n| Templates | First error |\n|--:|---|\n").unwrap();
        for (error, count) in errors.into_iter().take(10) {
            writeln!(text, "| {count} | {} |", error.replace('|', "\\|")).unwrap();
        }
    }

    text
}

#[test]
fn corpus() {
    let sources = read_sources();

    // Every template in the corpus, with its source.
    let mut jobs = vec![];
    for source in &sources {
        let checkout = fetch(source);
        for path in templates(source, &checkout) {
            jobs.push((source, checkout.join(&path), path.display().to_string()));
        }
    }

    // Check the templates in parallel, since each runs bracketlint three times.
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(4, |count| count.get());
    let mut results: BTreeMap<String, Outcomes> =
        sources.iter().map(|source| (source.name.clone(), Outcomes::new())).collect();

    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut outcomes = vec![];
                    while let Some((source, file, path)) =
                        jobs.get(next.fetch_add(1, Ordering::Relaxed))
                    {
                        outcomes.push((source, path, check(&source.dialect, file)));
                    }
                    outcomes
                })
            })
            .collect();

        for handle in handles {
            for (source, path, outcome) in handle.join().unwrap() {
                results.get_mut(&source.name).unwrap().insert(path.clone(), outcome);
            }
        }
    });

    let report = report(&sources, &results);
    println!("{report}");

    if std::env::var("REGENERATE_OUTPUT").is_ok_and(|value| value == "true") {
        let _ = fs::remove_dir_all(baselines_dir());
        fs::create_dir_all(baselines_dir()).unwrap();
        for (source, outcomes) in &results {
            fs::write(baselines_dir().join(format!("{source}.txt")), baseline(outcomes)).unwrap();
        }

        fs::write(corpus_dir().join("report.md"), &report).unwrap();
        return;
    }

    // Every template whose status differs from its baseline.
    let mut changes = vec![];
    for (source, outcomes) in &results {
        let baseline = read_baseline(source);
        for (path, outcome) in outcomes {
            match baseline.get(path) {
                Some(&before) if before == outcome.status => {}
                Some(&before) => changes.push(format!(
                    "{source}/{path}: {} -> {}{}",
                    before.name(),
                    outcome.status.name(),
                    if outcome.status < before { " (worse)" } else { "" }
                )),
                None => changes.push(format!("{source}/{path}: new, {}", outcome.status.name())),
            }
        }

        for path in baseline.keys().filter(|path| !outcomes.contains_key(*path)) {
            changes.push(format!("{source}/{path}: no longer in the corpus"));
        }
    }

    let recorded = fs::read_to_string(corpus_dir().join("report.md")).unwrap_or_default();
    if changes.is_empty() && recorded != report {
        changes.push("tests/corpus/report.md: its errors changed".to_string());
    }

    assert!(
        changes.is_empty(),
        "tests/corpus is out of date, record it with `just corpus-update`:\n{}",
        changes.join("\n")
    );
}
