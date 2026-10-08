//! What is recorded of how the corpus does: the status of every template of
//! each source in `baselines/`, and the report in `report.md`.

use std::{collections::BTreeMap, fmt::Write, fs, path::PathBuf};

use super::{
    check::{CheckedSource, Status},
    corpus_dir,
};

fn baselines_dir() -> PathBuf {
    corpus_dir().join("baselines")
}

fn baseline_path(source: &str) -> PathBuf {
    baselines_dir().join(format!("{source}.txt"))
}

fn report_path() -> PathBuf {
    corpus_dir().join("report.md")
}

/// Record how each template does in `results`, and `report`.
pub fn record(results: &[CheckedSource], report: &str) {
    // Start over, so that the baselines of removed sources go too.
    let _ = fs::remove_dir_all(baselines_dir());
    fs::create_dir_all(baselines_dir()).unwrap();

    for CheckedSource { source, outcomes } in results {
        let mut baseline = String::new();
        for (path, outcome) in outcomes {
            writeln!(baseline, "{:<13} {path}", outcome.status.name()).unwrap();
        }
        fs::write(baseline_path(&source.name), baseline).unwrap();
    }

    fs::write(report_path(), report).unwrap();
}

/// How `results` and `report` differ from what is recorded, a line for each
/// template whose status changed.
pub fn compare(results: &[CheckedSource], report: &str) -> Vec<String> {
    let mut changes = vec![];

    for CheckedSource { source, outcomes } in results {
        let name = &source.name;
        let baseline = read_baseline(name);

        for (path, outcome) in outcomes {
            let after = outcome.status;
            match baseline.get(path) {
                None => changes.push(format!("{name}/{path}: new, {}", after.name())),
                Some(&before) if before != after => {
                    let worse = if after < before { " (worse)" } else { "" };
                    changes.push(format!(
                        "{name}/{path}: {} -> {}{worse}",
                        before.name(),
                        after.name()
                    ));
                }
                Some(_) => {}
            }
        }

        for path in baseline.keys().filter(|path| !outcomes.contains_key(*path)) {
            changes.push(format!("{name}/{path}: no longer in the corpus"));
        }
    }

    // The statuses can stay the same while the errors that the report lists
    // change.
    let recorded = fs::read_to_string(report_path()).unwrap_or_default();
    if changes.is_empty() && recorded != report {
        changes.push("report.md: its errors changed".to_string());
    }

    changes
}

/// The status of each template of `source` in its baseline, by path.
fn read_baseline(source: &str) -> BTreeMap<String, Status> {
    let text = fs::read_to_string(baseline_path(source)).unwrap_or_default();

    text.lines()
        .map(|line| {
            let (status, path) = line.split_once(' ').unwrap();
            (path.trim().to_string(), status.parse().unwrap())
        })
        .collect()
}
