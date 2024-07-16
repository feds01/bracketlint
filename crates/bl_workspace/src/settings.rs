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
pub(crate) static EXCLUDE: &[FilePattern] = &[
];

pub(crate) static INCLUDE: &[FilePattern] = &[
];

#[derive(Debug, Clone, Default)]
pub struct FilePatternSet {
    /// The actual set of globs that are used to match files.
    set: GlobSet,

    _set_internals: Vec<FilePattern>,
}

impl FilePatternSet {
    pub fn try_from_iter<I>(patterns: I) -> Result<Self, anyhow::Error>
    where
        I: IntoIterator<Item = FilePattern>,
    {
        let mut builder = GlobSetBuilder::new();

        let mut _set_internals = vec![];

        for pattern in patterns {
            _set_internals.push(pattern.clone());
            pattern.add_to(&mut builder)?;
        }

        let set = builder.build()?;

        Ok(FilePatternSet { set, _set_internals })
    }
}

impl Deref for FilePatternSet {
    type Target = GlobSet;

    fn deref(&self) -> &Self::Target {
        &self.set
    }
}

/// The settings for the file resolver.
#[derive(Default)]
pub struct FileResolverSettings {
    /// Files that are explicitly included in the [super::Workspace].
    pub include: FilePatternSet,

    /// Files that are explicitly excluded from the [super::Workspace].
    pub exclude: FilePatternSet,

    /// Any user extensions to the exclusion patterns.
    pub user_exclude: FilePatternSet,

    /// Whether to enforce file exclusions.
    pub force_exclude: bool,
}

impl FileResolverSettings {
    pub fn new() -> Self {
        FileResolverSettings {
            include: FilePatternSet::try_from_iter(INCLUDE.iter().cloned()).unwrap(),
            exclude: FilePatternSet::try_from_iter(EXCLUDE.iter().cloned()).unwrap(),
            user_exclude: FilePatternSet::default(),
            force_exclude: false,
        }
    }
}
pub struct Settings {
    pub respect_gitignore: bool,

    /// Settings to do with file exclusions/inclusions.
    pub file_resolver: FileResolverSettings,
}

impl Settings {
    pub fn new(respect_gitignore: bool, fix_mode: FixMode) -> Self {
        Settings {
            respect_gitignore,
            file_resolver: FileResolverSettings::new(),
        }
    }
}
