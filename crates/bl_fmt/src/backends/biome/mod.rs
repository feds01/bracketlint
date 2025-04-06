//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use biome_diagnostics::DiagnosticExt;
use biome_html_parser::parse_html;
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use bl_ast::{ByteRange, SourceId};

use crate::{
    adapters::{
        ExternalLanguagesEngine, HasCssParsing, HasHtmlParsing, HasJsParsing, LanguageType,
        TerminalState,
    },
    diagnostics::{FmtError, FmtErrorKind, FmtResult},
};

/// A wrapper around the options for the Biome formatter. These options
/// represent the configurability of the formatter from the perspective of
/// `biome`.
pub struct BiomeFormatterOptions {
    /// The options for the HTML formatter.
    html: HtmlFormatOptions,
}

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
        Self {
            source,
            options: BiomeFormatterOptions {
                html: HtmlFormatOptions::default(),
            },
        }
    }
}

impl HasHtmlParsing for BiomeFormatter {
    /// Format the HTML contents using the Biome formatter.
    ///
    /// /// This will follow the algorithm:
    ///
    /// 1. Parse the HTML contents using the `biome_html_parser`.
    ///
    /// 2. If there are any errors, return them as a `FmtError`.
    ///
    ///
    /// 3. Format the parsed HTML using the `biome_html_formatter`.
    ///
    /// 4. Return the formatted HTML as a `String`.
    ///
    /// @@Todo: for (2 & 4) we may not want to do this, and simply return the
    /// contents as verbatim. We could emit an event for debugging purposes
    /// that parsing this content failed for some reason.
    fn format_html(&self, contents: &str) -> FmtResult<String> {
        let parsed = parse_html(contents);
        let language = LanguageType::Html;
        let mut errors = Vec::new();

        if parsed.has_errors() {
            for diagnostic in parsed.diagnostics() {
                let message = format!("{}", diagnostic.message);
                let err = diagnostic.clone().with_file_source_code("");

                // Extract this span from the error.
                errors.push(FmtError::new(
                    FmtErrorKind::ExternalLanguageParseError { language, message },
                    err.location().span.map(|text_range| {
                        bl_ast::Span::new(
                            ByteRange::new(text_range.start().into(), text_range.end().into()),
                            self.source,
                        )
                    }),
                ));
            }
        }

        let options = self.options.html.clone();

        match format_node(options, &parsed.syntax()) {
            Ok(formatted) => {
                // @@Temp: for now, just return the original contents.
                Ok(formatted.print().unwrap().into_code())
            }
            Err(_) => {
                Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language }, None))
            }
        }
    }

    fn terminal_state(&self) -> Option<TerminalState> {
        Some(TerminalState { language: LanguageType::Html })
    }
}
