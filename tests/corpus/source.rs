//! The sources of the corpus in `sources.toml`, whose templates are downloaded
//! into `target/corpus`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use bl_ast::Dialect;
use serde::Deserialize;

use super::{corpus_dir, target_dir};

/// A repository of templates at a pinned tag, of which only `paths` are
/// downloaded, and only the files with `extension` are checked in `dialect`.
#[derive(Deserialize)]
pub struct Source {
    pub name: String,
    pub dialect: Dialect,
    repo: String,
    tag: String,
    paths: Vec<String>,
    extension: String,
}

/// A template of a [Source].
pub struct Template {
    /// The path of the template in its source.
    pub path: String,

    /// The contents of the template.
    pub contents: String,
}

impl Source {
    /// The sources in `sources.toml`.
    pub fn read_all() -> Vec<Source> {
        #[derive(Deserialize)]
        struct Sources {
            source: Vec<Source>,
        }

        let text = fs::read_to_string(corpus_dir().join("sources.toml")).unwrap();
        toml::from_str::<Sources>(&text).expect("couldn't read tests/corpus/sources.toml").source
    }

    /// The templates of the source, sorted by path. They are downloaded unless
    /// they already are.
    pub fn templates(&self) -> Vec<Template> {
        let checkout = self.download();

        let mut files = vec![];
        for path in &self.paths {
            find_files(&checkout.join(path), &self.extension, &mut files);
        }
        files.sort();

        files
            .into_iter()
            .map(|file| Template {
                path: file.strip_prefix(&checkout).unwrap().display().to_string(),
                contents: fs::read_to_string(&file).unwrap(),
            })
            .collect()
    }

    /// Download the paths of the source at its tag into `target/corpus`,
    /// unless they already are, returning where they are.
    fn download(&self) -> PathBuf {
        let checkout = target_dir().join(format!("{}@{}", self.name, self.tag));

        // A checkout is only whole if git's files are there. CI's cache of
        // `target/` drops every file outside Cargo's own, keeping only the
        // directories, which would otherwise be taken for a checkout without
        // templates.
        if checkout.join(".git/HEAD").is_file() {
            return checkout;
        }
        let _ = fs::remove_dir_all(&checkout);

        // Download next to the checkout first, so that a download that fails
        // half way isn't taken for one.
        let partial = checkout.with_extension("partial");
        let _ = fs::remove_dir_all(&partial);
        let partial_path = partial.to_str().unwrap();

        #[rustfmt::skip]
        self.git(&[
            "clone", "--quiet", "--depth=1", "--filter=blob:none", "--sparse",
            "--branch", &self.tag, &self.repo, partial_path,
        ]);

        let mut sparse_checkout = vec!["-C", partial_path, "sparse-checkout", "set", "--no-cone"];
        sparse_checkout.extend(self.paths.iter().map(String::as_str));
        self.git(&sparse_checkout);

        fs::rename(&partial, &checkout).unwrap();
        checkout
    }

    /// Run git to download the source. Git warns that an annotated tag "is not
    /// a commit" in a shallow clone, so its output is only shown if it fails.
    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "advice.detachedHead=false"])
            .args(args)
            .output()
            .expect("couldn't run git");

        assert!(
            output.status.success(),
            "couldn't download {}:\n{}",
            self.name,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Find the files in `dir` with `extension`, recursively.
fn find_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_files(&path, extension, files);
        } else if path.extension().is_some_and(|ext| ext == extension) {
            files.push(path);
        }
    }
}
