use core::fmt;

use bl_utils::text::MaskNonPrinting;
use colored::Colorize;
use similar::{ChangeTag, TextDiff};

/// A representation of a difference between two strings.
///
/// This struct is used to compute and display the differences between
/// two strings, typically for the purpose of showing changes in
/// source code or text files. It uses the `similar` crate to compute
/// the differences and provides a formatted output for the changes.
pub struct Diff<'a> {
    /// The original source code.
    pub original: &'a str,

    /// The modified source code.
    pub modified: &'a str,

    /// A header for the diff, i.e. to indicate the file names or
    /// other relevant information.
    header: Option<(&'a str, &'a str)>,

    /// The computed differences between the original and modified
    /// source code.
    diff: TextDiff<'a, 'a, 'a, str>,

    /// A flag indicating whether to show a hint for a missing newline
    /// at the end of the file. This is useful for indicating that
    missing_newline_hint: bool,
}

impl<'a> Diff<'a> {
    pub fn new(original: &'a str, modified: &'a str) -> Self {
        let diff = TextDiff::from_lines(original, modified);
        Self { original, modified, header: None, diff, missing_newline_hint: true }
    }
}

impl fmt::Display for Diff<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((original, modified)) = self.header {
            writeln!(f, "--- {}", original.show_non_printing().red())?;
            writeln!(f, "+++ {}", modified.show_non_printing().green())?;
        }

        let mut unified = self.diff.unified_diff();

        unified.missing_newline_hint(self.missing_newline_hint);

        // Individual hunks (section of changes)
        for hunk in unified.iter_hunks() {
            writeln!(f, "{}", hunk.header())?;

            // individual lines
            for change in hunk.iter_changes() {
                let value = change.value().show_non_printing();
                match change.tag() {
                    ChangeTag::Equal => write!(f, " {value}")?,
                    ChangeTag::Delete => write!(f, "{}{}", "-".red(), value.red())?,
                    ChangeTag::Insert => write!(f, "{}{}", "+".green(), value.green())?,
                }

                if !self.diff.newline_terminated() {
                    writeln!(f)?;
                } else if change.missing_newline() {
                    if self.missing_newline_hint {
                        writeln!(f, "{}", "\n\\ No newline at end of file".red())?;
                    } else {
                        writeln!(f)?;
                    }
                }
            }
        }

        Ok(())
    }
}
