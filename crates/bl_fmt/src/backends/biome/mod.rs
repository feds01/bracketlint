/// A wrapper around the options for the Biome formatter. These options
/// represent the configurability of the formatter from the perspective of
/// `biome`.
pub struct BiomeFormatterOptions {}

/// The adapter implementation for the "Biome JS" formatter. This is a wrapper
/// around the `biome` crate's JS formatter, and provides a consistent interface
/// for formatting external languages like JS, CSS and HTML, which are embedded
/// within the template.
pub struct BiomeFormatter {
    /// The source of the module that is being formatted. This is mostly
    /// useful for diagnostics.
    source: SourceId,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl BiomeFormatter {
    pub fn new(source: SourceId) -> Self {
        Self { source, options: BiomeFormatterOptions {} }
    }
}
