//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssParserOptions};
use biome_diagnostics::DiagnosticExt;
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::parse_html;
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsFileSource, JsParserOptions};
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

    /// The options for the CSS formatter.
    css: CssFormatOptions,

    /// The options for the JS formatter.
    js: JsFormatOptions,
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
                css: CssFormatOptions::default(),
                // @@Temp: we might need to change this based on the source of the module, however
                // for now we can assume that is in-fact a script that we are formatting, since
                // we are selecting snippets from the templates.
                js: JsFormatOptions::new(JsFileSource::js_script()),
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

impl HasCssParsing for BiomeFormatter {
    /// Format the CSS contents using the Biome formatter.
    ///
    /// This will follow the algorithm:
    ///
    /// 1. Parse the CSS contents using the `biome_css_parser`.
    ///
    /// 2. If there are any errors, return them as a `FmtError`.
    ///
    /// 3. Format the parsed CSS using the `biome_css_formatter`.
    ///
    /// 4. Return the formatted CSS as a `String`.
    ///
    /// @@Todo: for (2 & 4) we may not want to do this, and simply return the
    /// contents as verbatim. We could emit an event for debugging purposes
    /// that parsing this content failed for some reason.
    fn format_css(&self, contents: &str) -> FmtResult<String> {
        // @@Todo: consider using `parse_css_with_cache` here, and store the cache
        // within our caching system.
        let parsed = css_parser::parse_css(
            contents,
            CssParserOptions::default().allow_wrong_line_comments().allow_metavariables(),
        );
        let language = LanguageType::Css;
        let mut diagnostics = vec![];
        let mut has_errors = false;

        for diagnostic in parsed.diagnostics() {
            has_errors |= diagnostic.is_error();

            let message = format!("{}", diagnostic.message);
            let err = diagnostic.clone().with_file_source_code("");

            // Extract this span from the error.
            diagnostics.push(FmtError::new(
                FmtErrorKind::ExternalLanguageParseError { language, message },
                err.location().span.map(|text_range| {
                    bl_ast::Span::new(
                        ByteRange::new(text_range.start().into(), text_range.end().into()),
                        self.source,
                    )
                }),
            ));
        }

        // If there are any errors, return them.
        if has_errors {
            return Err(FmtError::compound(diagnostics));
        }

        // Now, format the CSS.
        let options = self.options.css.clone();

        match css_formatter::format_node(options, &parsed.syntax()) {
            Ok(formatted) => {
                // @@Temp: for now, just return the original contents.
                Ok(formatted.print().unwrap().into_code())
            }
            Err(_) => {
                Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language }, None))
            }
        }
    }

    /// Returns the terminal state of the CSS formatter.
    ///
    /// Since a CSS block is a terminal "node" in the context of a template i.e.
    /// there may not be any other embedded languages within the CSS block,
    /// we can safely assume that the terminal state is `Css`.
    fn terminal_state(&self) -> Option<TerminalState> {
        Some(TerminalState { language: LanguageType::Css })
    }
}

impl HasJsParsing for BiomeFormatter {
    /// Format the JS contents using the Biome formatter.
    ///
    /// This will follow the algorithm:
    ///
    /// 1. Parse the JS contents using the `biome_js_parser`.
    ///
    /// 2. If there are any errors, return them as a `FmtError`.
    ///
    /// 3. Format the parsed JS using the `biome_js_formatter`.
    ///
    /// 4. Return the formatted JS as a `String`.
    ///
    /// @@Todo: for (2 & 4) we may not want to do this, and simply return the
    /// contents as verbatim. We could emit an event for debugging purposes
    /// that parsing this content failed for some reason.
    fn format_js(&self, contents: &str) -> FmtResult<String> {
        // @@Todo: consider using `parse_js_with_cache` here, and store the
        // cache within our caching system.
        let parsed = js_parser::parse_script(contents, JsParserOptions::default());

        let language = LanguageType::Js;
        let mut diagnostics = vec![];

        let mut has_errors = false;
        for diagnostic in parsed.diagnostics() {
            has_errors |= diagnostic.is_error();

            let message = format!("{}", diagnostic.message);
            let err = diagnostic.clone().with_file_source_code("");

            // Extract this span from the error.
            diagnostics.push(FmtError::new(
                FmtErrorKind::ExternalLanguageParseError { language, message },
                err.location().span.map(|text_range| {
                    bl_ast::Span::new(
                        ByteRange::new(text_range.start().into(), text_range.end().into()),
                        self.source,
                    )
                }),
            ));
        }

        // If there are any errors, return them.
        if has_errors {
            return Err(FmtError::compound(diagnostics));
        }

        // Now, format the JS.
        let options = self.options.js.clone();
        let formatted = js_formatter::format_node(options, &parsed.syntax());

        // @@Temp: for now, just return the original contents.
        match formatted {
            Ok(formatted) => Ok(formatted.print().unwrap().into_code()),
            Err(_) => {
                Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language }, None))
            }
        }
    }

    /// Returns the terminal state of the JS formatter.
    ///
    /// Since a JS block is a terminal "node" in the context of a template i.e.
    /// there may not be any other embedded languages within the JS block,
    /// we can safely assume that the terminal state is `Js`.
    fn terminal_state(&self) -> Option<TerminalState> {
        Some(TerminalState { language: LanguageType::Js })
    }
}
