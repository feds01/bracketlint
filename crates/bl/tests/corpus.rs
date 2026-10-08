//! Checks bracketlint against real-world templates in each dialect.
//!
//! The templates come from the [SOURCES] below, which are downloaded into
//! `target/corpus`. Each template gets the first of these statuses that
//! applies, from worst to best:
//!
//! - `parse-error`: `bracketlint check` reports an error.
//! - `fmt-error`: `bracketlint fmt` reports an error.
//! - `changed-tags`: formatting changed a `{% %}`, `{{ }}` or `{# #}` tag other
//!   than by its whitespace, i.e. it dropped or changed template source.
//! - `unstable`: formatting the formatted template changes it again.
//! - `ok`: none of the above.
//!
//! `tests/corpus/baseline.txt` records the status of every template, and the
//! test fails if any template's status differs from it, like a snapshot. Run
//! the test with `REGENERATE_OUTPUT=true` (`just corpus-update`) to record the
//! new statuses, so that support for each dialect only grows.

use std::{
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

/// A repository of templates at a pinned tag, of which only `paths` are
/// downloaded, and only files with `extension` are checked in `dialect`.
struct Source {
    name: &'static str,
    dialect: &'static str,
    repo: &'static str,
    tag: &'static str,
    paths: &'static [&'static str],
    extension: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        name: "django",
        dialect: "django",
        repo: "https://github.com/django/django.git",
        tag: "6.1.2",
        paths: &[
            "django/contrib/admin/templates",
            "django/contrib/admindocs/templates",
            "django/forms/templates",
            "django/views/templates",
        ],
        extension: "html",
    },
    Source {
        name: "sphinx",
        dialect: "jinja",
        repo: "https://github.com/sphinx-doc/sphinx.git",
        tag: "v9.1.0",
        paths: &["sphinx/themes/basic"],
        extension: "html",
    },
    // Only the HTML templates, since the LaTeX ones use other delimiters.
    Source {
        name: "nbconvert",
        dialect: "jinja",
        repo: "https://github.com/jupyter/nbconvert.git",
        tag: "v7.17.1",
        paths: &[
            "share/templates/base",
            "share/templates/basic",
            "share/templates/classic",
            "share/templates/lab",
            "share/templates/reveal",
        ],
        extension: "j2",
    },
    Source {
        name: "symfony",
        dialect: "twig",
        repo: "https://github.com/symfony/symfony.git",
        tag: "v8.1.8",
        paths: &[
            "src/Symfony/Bridge/Twig/Resources/views",
            "src/Symfony/Bundle/WebProfilerBundle/Resources/views",
        ],
        extension: "twig",
    },
    Source {
        name: "symfony-demo",
        dialect: "twig",
        repo: "https://github.com/symfony/demo.git",
        tag: "v3.1.0",
        paths: &["templates"],
        extension: "twig",
    },
    // Nunjucks is close to Jinja, and has no dialect of its own yet.
    Source {
        name: "govuk-frontend",
        dialect: "jinja",
        repo: "https://github.com/alphagov/govuk-frontend.git",
        tag: "v6.5.1",
        paths: &["packages/govuk-frontend/src/govuk"],
        extension: "njk",
    },
    Source {
        name: "eleventy-base-blog",
        dialect: "jinja",
        repo: "https://github.com/11ty/eleventy-base-blog.git",
        tag: "v9.0.0",
        paths: &["_includes", "content"],
        extension: "njk",
    },
    Source {
        name: "dawn",
        dialect: "liquid",
        repo: "https://github.com/Shopify/dawn.git",
        tag: "v16.0.0",
        paths: &["layout", "sections", "snippets", "blocks"],
        extension: "liquid",
    },
    Source {
        name: "minima",
        dialect: "liquid",
        repo: "https://github.com/jekyll/minima.git",
        tag: "v2.5.2",
        paths: &["_layouts", "_includes"],
        extension: "html",
    },
];

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
    dialect: &'static str,
    status: Status,
    error: Option<String>,
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Download the paths of `source` at its tag, unless they already are.
fn fetch(source: &Source) -> PathBuf {
    let target = root().join("target/corpus").join(format!("{}@{}", source.name, source.tag));
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
        source.tag,
        source.repo,
        partial_path,
    ]);
    git(&[&["-C", partial_path, "sparse-checkout", "set", "--no-cone"], source.paths].concat());

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
    for path in source.paths {
        walk(&checkout.join(path), source.extension, &mut files);
    }

    files.sort();
    files.into_iter().map(|file| file.strip_prefix(checkout).unwrap().to_path_buf()).collect()
}

/// Remove the ANSI escape codes that colour bracketlint's output.
fn strip_ansi(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut chars = text.chars();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            chars.by_ref().find(|&c| c == 'm');
        } else {
            stripped.push(c);
        }
    }

    stripped
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
fn check(dialect: &str, file: &Path) -> (Status, Option<String>) {
    let path = file.to_str().unwrap();
    if let Some(error) = bracketlint(&["check", "--dialect", dialect, path]).into_iter().next() {
        return (Status::ParseError, Some(error));
    }

    let scratch = tempfile::tempdir().unwrap();
    let copy = scratch.path().join(file.file_name().unwrap());
    fs::copy(file, &copy).unwrap();
    let copy_path = copy.to_str().unwrap();

    let format = || bracketlint(&["fmt", "--fix", "--dialect", dialect, copy_path]);
    if let Some(error) = format().into_iter().next() {
        return (Status::FmtError, Some(error));
    }

    let source = fs::read_to_string(file).unwrap();
    let formatted = fs::read_to_string(&copy).unwrap();
    let (before, after) = (tags(&source), tags(&formatted));
    if before != after {
        let changed = before.iter().zip(&after).find(|(old, new)| old != new);
        let error =
            changed.map_or("a tag was added or removed".to_string(), |(old, _)| old.clone());
        return (Status::ChangedTags, Some(error));
    }

    format();
    if fs::read_to_string(&copy).unwrap() != formatted {
        return (Status::Unstable, None);
    }

    (Status::Ok, None)
}

fn baseline_path() -> PathBuf {
    root().join("tests/corpus/baseline.txt")
}

fn read_baseline() -> BTreeMap<String, Status> {
    let text = fs::read_to_string(baseline_path()).unwrap_or_default();

    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (status, path) = line.split_once(' ').unwrap();
            (path.trim().to_string(), Status::from_name(status).unwrap())
        })
        .collect()
}

fn write_baseline(outcomes: &BTreeMap<String, Outcome>) {
    let mut text = String::from(
        "# The status of each template in the corpus, see crates/bl/tests/corpus.rs.\n",
    );
    for (path, outcome) in outcomes {
        writeln!(text, "{:<13} {path}", outcome.status.name()).unwrap();
    }

    fs::write(baseline_path(), text).unwrap();
}

/// A table of the statuses in each dialect, and its most common errors.
fn summary(outcomes: &BTreeMap<String, Outcome>) -> String {
    let mut counts: BTreeMap<&str, BTreeMap<Status, usize>> = BTreeMap::new();
    let mut errors: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();

    for outcome in outcomes.values() {
        *counts.entry(outcome.dialect).or_default().entry(outcome.status).or_default() += 1;

        if let (Status::ParseError | Status::FmtError, Some(error)) =
            (outcome.status, &outcome.error)
        {
            *errors.entry(outcome.dialect).or_default().entry(error).or_default() += 1;
        }
    }

    let mut text = format!("{:<8} {:>5}", "dialect", "files");
    for status in Status::ALL {
        write!(text, "  {:>12}", status.name()).unwrap();
    }

    for (dialect, counts) in &counts {
        write!(text, "\n{dialect:<8} {:>5}", counts.values().sum::<usize>()).unwrap();
        for status in Status::ALL {
            write!(text, "  {:>12}", counts.get(&status).unwrap_or(&0)).unwrap();
        }
    }

    for (dialect, errors) in &errors {
        let mut errors: Vec<_> = errors.iter().collect();
        errors.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));

        write!(text, "\n\nMost common errors in {dialect}:").unwrap();
        for (error, count) in errors.into_iter().take(8) {
            write!(text, "\n  {count:>4}  {error}").unwrap();
        }
    }

    text
}

#[test]
fn corpus() {
    // Every template in the corpus, as its source and path.
    let mut jobs = vec![];
    for source in SOURCES {
        let checkout = fetch(source);
        for path in templates(source, &checkout) {
            jobs.push((
                source,
                checkout.join(&path),
                format!("{}/{}", source.name, path.display()),
            ));
        }
    }

    // Check the templates in parallel, since each runs bracketlint three times.
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(4, |count| count.get());
    let outcomes: BTreeMap<String, Outcome> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut outcomes = vec![];
                    while let Some((source, file, key)) =
                        jobs.get(next.fetch_add(1, Ordering::Relaxed))
                    {
                        let (status, error) = check(source.dialect, file);
                        outcomes.push((
                            key.clone(),
                            Outcome { dialect: source.dialect, status, error },
                        ));
                    }
                    outcomes
                })
            })
            .collect();

        handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect()
    });

    let summary = summary(&outcomes);
    println!("{summary}");

    if std::env::var("REGENERATE_OUTPUT").is_ok_and(|value| value == "true") {
        write_baseline(&outcomes);
        return;
    }

    // Every template whose status differs from the baseline.
    let baseline = read_baseline();
    let mut changes = vec![];
    for (path, outcome) in &outcomes {
        match baseline.get(path) {
            Some(&before) if before == outcome.status => {}
            Some(&before) => changes.push(format!(
                "{path}: {} -> {}{}",
                before.name(),
                outcome.status.name(),
                if outcome.status < before { " (worse)" } else { "" }
            )),
            None => changes.push(format!("{path}: new, {}", outcome.status.name())),
        }
    }
    for path in baseline.keys().filter(|path| !outcomes.contains_key(*path)) {
        changes.push(format!("{path}: no longer in the corpus"));
    }

    assert!(
        changes.is_empty(),
        "{summary}\n\n{} templates differ from tests/corpus/baseline.txt, record them with `just \
         corpus-update`:\n{}",
        changes.len(),
        changes.join("\n")
    );
}
