//! Path manipulation and canonicalisation utilities.

use std::{
    env,
    fs::canonicalize,
    path::{Path, PathBuf},
    sync::LazyLock,
};

/// The path of `path` from the current directory, as a user names it, or
/// `path` itself if it isn't in the current directory.
///
/// ##Note: The current directory is read once, and kept for the rest of the
/// run. Reading it opens the directory on macOS, and `fmt --check` names each
/// file that it would reformat with this.
pub fn relative_path(path: &Path) -> &Path {
    static CURRENT_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| env::current_dir().ok());
    CURRENT_DIR.as_deref().and_then(|dir| path.strip_prefix(dir).ok()).unwrap_or(path)
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
