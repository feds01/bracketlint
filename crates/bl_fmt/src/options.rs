//! Options for the formatter.

use bl_workspace::config::Markup;

/// Various options for the formatter.
#[derive(Debug, Clone, Copy)]
pub struct FormatterOptions {
    /// The size of the indent in spaces.
    pub indent_size: u8,

    /// What the template outputs, which decides what the formatter can change
    /// between its tags.
    pub markup: Markup,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self { indent_size: 4, markup: Markup::default() }
    }
}
