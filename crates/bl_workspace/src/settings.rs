//! Defines all of the settings that a [super::Workspace] can hold.

use std::{ops::Deref, path::PathBuf, str::FromStr};
use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum FilePattern {
    Builtin(&'static str),
    User(String, PathBuf),
}
impl FilePattern {
    pub fn add_to(self, builder: &mut GlobSetBuilder) -> Result<()> {
        match self {
            FilePattern::Builtin(pattern) => {
                builder.add(Glob::from_str(pattern)?);
            }
            FilePattern::User(pattern, absolute) => {
                // Add the absolute path.
                builder.add(Glob::new(&absolute.to_string_lossy())?);

                // Add basename path.
                if !pattern.contains(std::path::MAIN_SEPARATOR) {
                    builder.add(Glob::new(&pattern)?);
                }
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default)]
pub struct FilePatternSet {
    /// The actual set of globs that are used to match files.
    set: GlobSet,

    _set_internals: Vec<FilePattern>,
}
impl Deref for FilePatternSet {
    type Target = GlobSet;

    fn deref(&self) -> &Self::Target {
        &self.set
    }
}
