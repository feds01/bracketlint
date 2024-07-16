//! Various structures and options for the linting system.

/// When a lint option is detected, this is the method of communicating the fix,
/// whether it is simply displaying the fix, applying it to the file, or
/// generating a new file with the fix.
#[derive(Debug, Clone, Copy, Default)]
pub enum FixMode {
    #[default]
    Diff,

    Generate,

    Apply,
}
