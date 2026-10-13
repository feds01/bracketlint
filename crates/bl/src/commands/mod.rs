//! Defines all of the commands that are available to the user.

pub mod check;
pub mod fmt;
pub mod version;

use std::{env, path::Path};

/// The path of `path` from the current directory, as a user names it, or
/// `path` itself if it isn't in the current directory.
pub(crate) fn relative_path(path: &Path) -> &Path {
    env::current_dir().ok().and_then(|dir| path.strip_prefix(dir).ok()).unwrap_or(path)
}
