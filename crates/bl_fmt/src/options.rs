//! Options for the formatter.

/// Various options for the formatter.
#[derive(Debug, Clone, Copy)]
pub struct FormatterOptions {
    /// The size of the indent in spaces.
    pub indent_size: u8,
}

impl Default for FormatterOptions {
    fn default() -> Self {
        Self { indent_size: 4 }
    }
}
