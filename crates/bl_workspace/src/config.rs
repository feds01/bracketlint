//! The settings of a project, which its `bracketlint.toml` holds, e.g.
//!
//! ```toml
//! dialect = "jinja"
//! markup = "text"
//! ```

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use bl_ast::Dialect;
use serde::Deserialize;

/// The name of the file that holds the settings of a project.
pub const CONFIG_FILE: &str = "bracketlint.toml";

/// What the templates of a project output, which decides what the formatter
/// can change between their tags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Markup {
    /// HTML, in which a run of whitespace between words is shown as a single
    /// space, so the formatter indents the text and wraps its lines.
    #[default]
    Html,

    /// Text, e.g. Markdown or a prompt for a language model, whose whitespace
    /// is output as it is written. The formatter keeps all of it, and only
    /// formats the tags.
    Text,
}

/// The settings in a project's `bracketlint.toml`. Any that it leaves out
/// keeps its default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    /// The dialect of the templates whose extension doesn't name one, e.g.
    /// `.html`, which are otherwise read as Django. `--dialect` still reads
    /// every file in the dialect that it names.
    pub dialect: Option<Dialect>,

    /// What the templates output.
    #[serde(default)]
    pub markup: Markup,
}

impl ProjectConfig {
    /// The settings in the first `bracketlint.toml` in `dir` or any of its
    /// parents, with the path that they are read from, or the defaults if
    /// there is none.
    pub fn discover(dir: &Path) -> Result<(Self, Option<PathBuf>)> {
        let Some(path) = dir.ancestors().map(|dir| dir.join(CONFIG_FILE)).find(|p| p.is_file())
        else {
            return Ok((Self::default(), None));
        };

        Ok((Self::read(&path)?, Some(path)))
    }

    /// The settings in the `bracketlint.toml` at `path`.
    pub fn read(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("couldn't read `{}`", path.display()))?;

        Self::parse(&contents).with_context(|| format!("`{}` isn't valid", path.display()))
    }

    /// The settings in `contents`, the TOML of a `bracketlint.toml`.
    pub fn parse(contents: &str) -> Result<Self> {
        Ok(toml::from_str(contents)?)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn settings_default_when_left_out() {
        assert_eq!(ProjectConfig::parse("").unwrap(), ProjectConfig::default());

        let config = ProjectConfig::parse("dialect = \"jinja\"\nmarkup = \"text\"\n").unwrap();
        assert_eq!(config, ProjectConfig { dialect: Some(Dialect::Jinja), markup: Markup::Text });
    }

    #[test]
    fn unknown_settings_and_values_are_errors() {
        let error = ProjectConfig::parse("markup = \"markdown\"").unwrap_err().to_string();
        assert!(error.contains("unknown variant `markdown`"), "{error}");

        let error = ProjectConfig::parse("dialects = \"jinja\"").unwrap_err().to_string();
        assert!(error.contains("unknown field `dialects`"), "{error}");
    }

    #[test]
    fn discovered_in_the_closest_parent() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("templates/emails");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(ProjectConfig::discover(&nested).unwrap(), (ProjectConfig::default(), None));

        let path = root.path().join(CONFIG_FILE);
        fs::write(&path, "markup = \"text\"").unwrap();
        let (config, found) = ProjectConfig::discover(&nested).unwrap();
        assert_eq!((config.markup, found), (Markup::Text, Some(path)));
    }
}
