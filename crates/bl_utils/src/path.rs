//! Path manipulation and canonicalisation utilities.

use std::{
    env,
    fs::canonicalize,
    path::{Path, PathBuf},
};

/// The path of `path` from the current directory, as a user names it, or
/// `path` itself if it isn't in the current directory.
pub fn relative_path(path: &Path) -> &Path {
    env::current_dir().ok().and_then(|dir| path.strip_prefix(dir).ok()).unwrap_or(path)
}

/// Function to apply formatting onto a path when printing it.
#[cfg(not(target_os = "windows"))]
pub fn adjust_canonicalisation<P: AsRef<Path>>(p: P) -> PathBuf {
    canonicalize(p.as_ref()).unwrap_or_else(|_| p.as_ref().to_path_buf())
}

/// Function to apply formatting onto a path when printing it.
#[cfg(target_os = "windows")]
pub fn adjust_canonicalisation<P: AsRef<Path>>(p: P) -> PathBuf {
    const VERBATIM_PREFIX: &str = r"\\?\";
    let path = canonicalize(p.as_ref()).unwrap_or_else(|_| p.as_ref().to_path_buf());

    if let Ok(stripped_path) = path.strip_prefix(VERBATIM_PREFIX) {
        stripped_path.to_path_buf()
    } else {
        path
    }
}
