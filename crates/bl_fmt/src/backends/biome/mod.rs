//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use std::{
    convert::Infallible,
    ops::{FromResidual, Try},
};

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssParserOptions};
use biome_diagnostics::DiagnosticExt;
use biome_formatter::IndentStyle;
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::parse_html;
use biome_html_syntax::{HtmlElementList, HtmlRoot};
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsFileSource, JsParserOptions};
use biome_rowan::{AstNode, AstNodeList};
use bl_ast::{ByteRange, SpannedSource};

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
                // @@Todo: we need to have this be shared across our formatting configuration.
                html: HtmlFormatOptions::default()
                    .with_indent_width(4.try_into().unwrap())
                    .with_indent_style(IndentStyle::Space),
                css: CssFormatOptions::default(),
                // @@Temp: we might need to change this based on the source of the module, however
                // for now we can assume that is in-fact a script that we are formatting, since
                // we are selecting snippets from the templates.
                js: JsFormatOptions::new(JsFileSource::js_script()),
            },
        }
    }
}

struct HTMLBiomeFormatter<'ctx> {
    context: &'ctx FormatterContext<'ctx>,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,

    /// The state of the formatter, which is used to determine the
    /// terminal state of the formatter.
    state: TerminalState,
}

impl<'ctx> HTMLBiomeFormatter<'ctx> {
    /// A utility function to get the rightmost child of a node.
    ///
    /// This is used to determine the last child of a node, which is useful for
    /// determining the terminal state of the parser.
    fn apply_state_from_tree(&mut self, tree: &HtmlRoot) {
        let options = &self.options.html;
        let html = tree.html();

        // If we don't have any children, but we do have an EOF token, we can
        // try and infer some context from the EOF token.
        if html.iter().last().is_none()
            && let Ok(_) = tree.eof_token()
        {
            let end: usize = html.range().end().into();
            let contents_line_end = self.context.source().line_ranges.line_end(end);
            if end != contents_line_end {
                return;
            }

            self.state.indent = self.state.indent.saturating_sub(self.context.indent_step() as u16);
            return;
        }

        let TerminalCalculationState::Some { language, indent } =
            find_rightmost_child_and_extract_state(options, &html, self.context.source())
        else {
            return;
        };

        // If we've got an indent to apply, we need to apply it to the
        // formatter state.
        if let Some(indent) = indent {
            let current_indent = self.state.indent as i8;
            let indent = current_indent.saturating_add(indent);
            self.state.indent = indent as u16;
        }

        self.state.language = language;
    }
}

impl<'ctx> HasHTMLParsing<'ctx> for HTMLBiomeFormatter<'ctx> {
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
        let tree = parsed.tree();

        // In order to compute the terminal state, we're going to check
        // the top most node of the HTML document, and remember the value
        // so that we can inspect, the following traits:
        //
        // - What language did we "finish" with?
        //    - If `style``, then we must be in a CSS block.
        //    - If `script`, then we must be in a JS block.
        //    - Otherwise, we are in a HTML block.
        self.apply_state_from_tree(&tree);

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
                            self.context.id(),
                        )
                    }),
                ));
            }
        }

        let options = self.options.html.clone();

        match format_node(options, &parsed.syntax()) {
            Ok(formatted) => {
                let printed = formatted.print().unwrap();
                Ok(printed.into_code())
            }
            Err(_) => {
                Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language }, None))
            }
        }
    }

    /// Returns the terminal state of the HTML formatter.
    fn into_state(self) -> TerminalState {
        self.state
    }
}

enum TerminalCalculationState {
    None,
    Some {
        language: LanguageType,

        /// Any indentation that was pending.
        indent: Option<i8>,
    },
    UseParent,
}

impl Try for TerminalCalculationState {
    type Output = TerminalCalculationState;
    type Residual = TerminalCalculationState;

    fn from_output(output: Self::Output) -> Self {
        output
    }

    fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
        match self {
            // Define which values should short-circuit when using ?
            TerminalCalculationState::UseParent => std::ops::ControlFlow::Break(self),

            // Define which values represent "success" and should continue execution
            val @ (TerminalCalculationState::None | TerminalCalculationState::Some { .. }) => {
                std::ops::ControlFlow::Continue(val)
            }
        }
    }
}

impl FromResidual for TerminalCalculationState {
    fn from_residual(residual: <Self as Try>::Residual) -> Self {
        residual
    }
}

impl<E> FromResidual<Result<Infallible, E>> for TerminalCalculationState {
    fn from_residual(_: Result<Infallible, E>) -> Self {
        // When Result::Err is encountered with ?, return TerminalCalculationState::None
        TerminalCalculationState::None
    }
}

fn find_rightmost_child_and_extract_state(
    options: &HtmlFormatOptions,
    node: &HtmlElementList,
    spanned: SpannedSource<'_>,
) -> TerminalCalculationState {
    if let Some(child) = node.into_iter().last() {
        let html_element = match child {
            // If the child is an HTML element, we need to check if it has any
            // children.
            biome_html_syntax::AnyHtmlElement::HtmlElement(e) => e,

            // biome_html_syntax::AnyHtmlElement::HtmlSelfClosingElement(e) => {
            //     let indent = options.indent_width().value().wrapping_sub(2); // @@Temp: Hardcoded
            //     return TerminalCalculationState::Some {
            //         language: LanguageType::Html,
            //         indent: Some(indent),
            //     };
            // }

            // If we get an element that is auxiliary, it means that we can
            // backtrack and in fact use the parent element which should be
            // the last "tag" child.
            _ => return TerminalCalculationState::UseParent,
        };

        let nodes = html_element.children();

        if !nodes.is_empty() {
            // Handle the case where the child is an HTML element, and it has
            // children.
            match find_rightmost_child_and_extract_state(options, &nodes, spanned) {
                TerminalCalculationState::UseParent => {}
                state => return state,
            }
        }

        let opening_element = html_element.opening_element()?;
        let name_token = opening_element.name()?.value_token()?;

        let size = options.indent_width().value() as i8;

        let indent = match html_element.closing_element() {
            Some(_) => Some(-size),
            None => {
                // @@CrazyHueristic: if the name is not a self-closing element, and it uses all
                // of the space on the current line, we can assume that we should
                // increase the indent level by 1.
                let range = opening_element.range();
                let end: usize = range.end().into();
                let contents_line_end = spanned.line_ranges.line_end(end);

                if end == contents_line_end { Some(size) } else { None }
            }
        };

        let language = match name_token.text().trim() {
            "style" => LanguageType::Css,
            "script" => LanguageType::Js,
            _ => LanguageType::Html,
        };

        TerminalCalculationState::Some { language, indent }
    } else {
        TerminalCalculationState::None
    }
}

struct CSSBiomeFormatter<'ctx> {
    context: &'ctx FormatterContext<'ctx>,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl<'ctx> HasCSSParsing<'ctx> for CSSBiomeFormatter<'ctx> {
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
                        self.context.id(),
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
        Some(TerminalState { language: LanguageType::Css, indent: 0, continue_inline: false })
    }
}

struct JSBiomeFormatter<'ctx> {
    context: &'ctx FormatterContext<'ctx>,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl<'ctx> HasJSParsing<'ctx> for JSBiomeFormatter<'ctx> {
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
                        self.context.id(),
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
        Some(TerminalState { language: LanguageType::Js, indent: 0, continue_inline: false })
    }
}

// We've implemented the `ExternalLanguagesEngine` trait for the
// `BiomeFormatter` by implementing the `HasHtmlParsing`, `HasCssParsing` and
// `HasJsParsing` traits.
impl ExternalLanguagesEngineAdaptor for BiomeFormatter {
    type CSSEngine<'ctx> = impl HasCSSParsing<'ctx>;
    type HTMLEngine<'ctx> = impl HasHTMLParsing<'ctx>;
    type JSEngine<'ctx> = impl HasJSParsing<'ctx>;

    fn html_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::HTMLEngine<'ctx> {
        HTMLBiomeFormatter { context, options: self.options.clone(), state: context.state }
    }

    fn css_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::CSSEngine<'ctx> {
        CSSBiomeFormatter { context, options: self.options.clone() }
    }

    fn js_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::JSEngine<'ctx> {
        JSBiomeFormatter { context, options: self.options.clone() }
    }
}
