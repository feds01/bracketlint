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
