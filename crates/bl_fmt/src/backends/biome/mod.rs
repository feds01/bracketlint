//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssParserOptions};
use biome_diagnostics::DiagnosticExt;
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::parse_html;
use biome_html_syntax::HtmlElementList;
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsFileSource, JsParserOptions};
use biome_rowan::AstNodeList;
use bl_ast::ByteRange;

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, FormatterContext, HasCSSParsing, HasHTMLParsing,
        HasJSParsing, LanguageType, TerminalState,
    },
    diagnostics::{FmtError, FmtErrorKind, FmtResult},
};

/// A wrapper around the options for the Biome formatter. These options
/// represent the configurability of the formatter from the perspective of
/// `biome`.
#[derive(Debug, Clone)]
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
    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl BiomeFormatter {
    pub fn new() -> Self {
        Self {
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

struct HTMLBiomeFormatter {
    context: FormatterContext,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,

    state: Option<TerminalState>,
}

impl HasHTMLParsing for HTMLBiomeFormatter {
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
    fn format(&mut self, contents: &str) -> FmtResult<String> {
        let parsed = parse_html(contents);
        let html = parsed.tree().html();

        // In order to compute the terminal state, we're going to check
        // the top most node of the HTML document, and remember the value
        // so that we can inspect, the following traits:
        //
        // - What language did we "finish" with?
        //    - If `style``, then we must be in a CSS block.
        //    - If `script`, then we must be in a JS block.
        //    - Otherwise, we are in a HTML block.
        self.state = terminal_state_from_rightmost_child(&html);

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
                            self.context.source(),
                        )
                    }),
                ));
            }
        }

        let options = self.options.html.clone();

        match format_node(options, &parsed.syntax()) {
            Ok(formatted) => {
                let indent = self.context.indent();
                Ok(formatted.print_with_indent(indent).unwrap().into_code())
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

struct CSSBiomeFormatter {
    context: FormatterContext,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl HasCSSParsing for CSSBiomeFormatter {
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
    fn format(&self, contents: &str) -> FmtResult<String> {
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
                        self.context.source(),
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

enum TerminalCalculationState {
    None,
    Some(TerminalState),
    UseParent,
}

/// A utility function to get the rightmost child of a node.
///
/// This is used to determine the last child of a node, which is useful for
/// determining the terminal state of the parser.
fn terminal_state_from_rightmost_child(node: &HtmlElementList) -> Option<TerminalState> {
    match find_rightmost_child_and_extract_state(node) {
        TerminalCalculationState::Some(state) => Some(state),
        _ => None,
    }
}

fn find_rightmost_child_and_extract_state(node: &HtmlElementList) -> TerminalCalculationState {
    if let Some(child) = node.into_iter().last() {
        // If the child is an HTML element, we need to check if it has any
        // children.
        let biome_html_syntax::AnyHtmlElement::HtmlElement(html_element) = child else {
            // If we get an element that is auxiliary, it means that we can
            // backtrack and in fact use the parent element which should be
            // the last "tag" child.
            return TerminalCalculationState::UseParent;
        };

        let nodes = html_element.children();

        if !nodes.is_empty() {
            // Handle the case where the child is an HTML element, and it has
            // children.
            match find_rightmost_child_and_extract_state(&nodes) {
                TerminalCalculationState::UseParent => {}
                state => return state,
            }
        }

        // @@Todo: clean this up so that we can gracefully handle these
        // unwraps.
        let name_token =
            html_element.opening_element().unwrap().name().unwrap().value_token().unwrap();

        match name_token.text().trim() {
            "style" => {
                TerminalCalculationState::Some(TerminalState { language: LanguageType::Css })
            }
            "script" => {
                TerminalCalculationState::Some(TerminalState { language: LanguageType::Js })
            }
            _ => TerminalCalculationState::Some(TerminalState { language: LanguageType::Html }),
        }
    } else {
        TerminalCalculationState::None
    }
}

struct JSBiomeFormatter {
    context: FormatterContext,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl HasJSParsing for JSBiomeFormatter {
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
    fn format(&self, contents: &str) -> FmtResult<String> {
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
                        self.context.source(),
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

// We've implemented the `ExternalLanguagesEngine` trait for the
// `BiomeFormatter` by implementing the `HasHtmlParsing`, `HasCssParsing` and
// `HasJsParsing` traits.
impl ExternalLanguagesEngineAdaptor for BiomeFormatter {
    type HTMLEngine = impl HasHTMLParsing;
    type CSSEngine = impl HasCSSParsing;
    type JSEngine = impl HasJSParsing;

    fn html_engine(&self, context: &FormatterContext) -> Self::HTMLEngine {
        HTMLBiomeFormatter { context: context.clone(), options: self.options.clone(), state: None }
    }

    fn css_engine(&self, context: &FormatterContext) -> Self::CSSEngine {
        CSSBiomeFormatter { context: context.clone(), options: self.options.clone() }
    }

    fn js_engine(&self, context: &FormatterContext) -> Self::JSEngine {
        JSBiomeFormatter { context: context.clone(), options: self.options.clone() }
    }
}
