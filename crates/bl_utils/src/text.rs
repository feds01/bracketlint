//! Collections of textual utilities for `bracketlint`.
//!
//! This module contains various utilities for working with text, including
//! formatting, printing, filtering, and other text-related operations.

use std::borrow::Cow;

/// A trait for masking non-printing characters in a string.
///
/// This trait provides a method to replace non-printing characters with
/// their corresponding visual representations. The method returns a
/// `Cow` (Clone on Write) reference to the modified string, which can
/// either be a borrowed reference or an owned string, depending on
/// whether the string was modified or not.
pub trait MaskNonPrinting {
    fn show_non_printing(&self) -> Cow<'_, str>;
}

macro_rules! impl_show_non_printing {
    ($(($from:expr, $to:expr)),+) => {
        impl MaskNonPrinting for str {
            fn show_non_printing(&self) -> Cow<'_, str> {
                if self.find(&[$($from),*][..]).is_some() {
                    Cow::Owned(
                        self.$(replace($from, $to)).*
                    )
                } else {
                    Cow::Borrowed(self)
                }
            }
        }
    };
}

impl_show_non_printing!(('\x07', "␇"), ('\x08', "␈"), ('\x1b', "␛"), ('\x7f', "␡"));

/// Remove the ANSI escape codes that colour text, e.g. bracketlint's
/// diagnostics, i.e. every sequence from `\x1b` up to the `m` that ends it.
pub fn strip_ansi(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut chars = text.chars();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            chars.by_ref().find(|&c| c == 'm');
        } else {
            stripped.push(c);
        }
    }

    stripped
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[1m\x1b[91merror\x1b[0m: oops"), "error: oops");
        assert_eq!(strip_ansi("plain"), "plain");
    }
}
