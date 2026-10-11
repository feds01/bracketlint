//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use std::{
    cell::RefCell,
    convert::Infallible,
    ops::{FromResidual, Residual, Try},
    rc::Rc,
};

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssParserOptions};
use biome_formatter::{FormatError, IndentStyle};
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::{HtmlParserOptions, parse_html_with_cache};
use biome_html_syntax::{HtmlElementList, HtmlRoot, HtmlSyntaxNode, T};
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsParserOptions};
use biome_languages::{CssFileSource, HtmlFileSource, JsFileSource};
use biome_rowan::{AstNode, AstNodeList, Direction, NodeCache, SyntaxKind};
use bl_ast::SpannedSource;

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, FormatterContext, HasCSSParsing, HasHTMLParsing,
        HasJSParsing, LanguageType, TerminalState, VERBATIM_ELEMENTS,
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

    /// The cache of the nodes and tokens that the HTML parser makes, which is
    /// shared by each piece of HTML in the template.
    html_cache: Rc<RefCell<NodeCache>>,
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
            html_cache: Rc::default(),
        }
    }
}

struct HTMLBiomeFormatter<'ctx> {
    context: &'ctx FormatterContext<'ctx>,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,

    /// The cache of the nodes and tokens that the HTML parser makes.
    cache: Rc<RefCell<NodeCache>>,

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
            self.state.indent = self.state.indent.saturating_add_signed(indent.into());
        }

        self.state.language = language;
    }
}

impl<'ctx> HasHTMLParsing<'ctx> for HTMLBiomeFormatter<'ctx> {
    /// Format the HTML contents using the Biome formatter, by parsing them
    /// with `biome_html_parser` and formatting the tree with
    /// `biome_html_formatter`.
    ///
    /// ##Note: The contents are a piece of the template between its tags,
    /// which often isn't valid HTML on its own. Elements that the piece opens
    /// or closes without the other are formatted all the same, but a piece
    /// where a template tag cuts an HTML tag, e.g. `<li class="`, is kept as
    /// it is written, see [has_broken_tag].
    fn format(&mut self, contents: &str) -> FmtResult<Option<String>> {
        let options = HtmlParserOptions::from(&HtmlFileSource::html());
        let parsed = parse_html_with_cache(contents, &mut self.cache.borrow_mut(), options);
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

        if has_broken_tag(&parsed.syntax()) {
            return Ok(None);
        }

        let language = LanguageType::Html;
        let options = self.options.html.clone();

        match format_node(options, &parsed.syntax(), vec![]) {
            Ok(formatted) => {
                let printed = formatted.print().unwrap();
                Ok(Some(printed.into_code()))
            }
            // A tag without its `>`, e.g. `<div ` before `{{ attributes }}`,
            // has no node that Biome can format.
            Err(FormatError::SyntaxError) => Ok(None),
            Err(_) => Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language })),
        }
    }

    /// Returns the terminal state of the HTML formatter.
    fn into_state(self) -> TerminalState {
        self.state
    }
}

/// Whether Biome couldn't parse a tag in the HTML of `root`, e.g. `<li class="`
/// that a template tag cuts. Biome prints what it couldn't parse as it is
/// written, among the HTML that it lays out, so that formatting the piece again
/// lays it out differently. For one, the other lines of such a tag keep the
/// indentation of the source, which the formatter would indent again each run.
///
/// A closing tag whose element opens before a template tag, e.g. the `</li>` in
/// `{% endif %}</li>`, is parsed into a bogus element as well. It is printed as
/// any other closing tag, unless it spans several lines, e.g. `</li\n>`, or it
/// closes one of the [VERBATIM_ELEMENTS], whose content Biome would then lay
/// out as other HTML, since it only knows it from their opening tag.
fn has_broken_tag(root: &HtmlSyntaxNode) -> bool {
    let is_closing_tag = |node: &HtmlSyntaxNode| {
        let mut tokens = node.descendants_tokens(Direction::Next);
        let (Some(open), Some(slash), Some(name)) = (tokens.next(), tokens.next(), tokens.next())
        else {
            return false;
        };

        let name = name.text_trimmed();
        open.kind() == T![<]
            && slash.kind() == T![/]
            && !VERBATIM_ELEMENTS.iter().any(|element| name.eq_ignore_ascii_case(element))
            && !node.text_trimmed().contains_char('\n')
    };

    root.descendants().any(|node| node.kind().is_bogus() && !is_closing_tag(&node))
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

impl Residual<TerminalCalculationState> for TerminalCalculationState {
    type TryType = TerminalCalculationState;
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
        let Some(name) = opening_element.tag_name() else {
            return TerminalCalculationState::None;
        };

        let size = options.indent_width().value() as i8;

        let indent = match html_element.closing_element() {
            Some(_) => Some(-size),
            None => {
                // @@CrazyHeuristic: if the name is not a self-closing element, and it uses all
                // of the space on the current line, we can assume that we should
                // increase the indent level by 1.
                let range = opening_element.range();
                let end: usize = range.end().into();
                let contents_line_end = spanned.line_ranges.line_end(end);

                if end == contents_line_end { Some(size) } else { None }
            }
        };

        // The text after a `<style>` or `<script>` element is CSS or JavaScript
        // only while the element is open, e.g. when a tag in its body splits
        // it. Once it is closed, the text after it is HTML again.
        let element = name.text();
        let is_open = html_element.closing_element().is_none();
        let language = if is_open { LanguageType::of_element(element) } else { LanguageType::Html };

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
    /// Format the CSS contents using the Biome formatter, by parsing them with
    /// `biome_css_parser` and formatting the tree with `biome_css_formatter`.
    ///
    /// ##Note: When a tag splits a `<style>` element, its contents are pieces
    /// of CSS, e.g. `#Banner-` before `{{ section.id }}`, which often aren't
    /// valid on their own. When they don't parse, they are kept as they are
    /// written.
    fn format(&self, contents: &str) -> FmtResult<Option<String>> {
        // @@Todo: consider using `parse_css_with_cache` here, and store the cache
        // within our caching system.
        let parsed = css_parser::parse_css(
            contents,
            CssFileSource::css(),
            CssParserOptions::default().allow_wrong_line_comments().allow_metavariables(),
        );
        if parsed.has_errors() {
            return Ok(None);
        }

        let language = LanguageType::Css;
        let options = self.options.css.clone();

        match css_formatter::format_node(options, &parsed.syntax()) {
            Ok(formatted) => Ok(Some(formatted.print().unwrap().into_code())),
            Err(_) => Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language })),
        }
    }

    /// Returns the terminal state of the CSS formatter.
    ///
    /// Since a CSS block is a terminal "node" in the context of a template i.e.
    /// there may not be any other embedded languages within the CSS block,
    /// we can safely assume that the terminal state is `Css`, at the same
    /// indent.
    fn into_state(self) -> TerminalState {
        TerminalState { language: LanguageType::Css, ..self.context.state }
    }
}

struct JSBiomeFormatter<'ctx> {
    context: &'ctx FormatterContext<'ctx>,

    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,
}

impl<'ctx> HasJSParsing<'ctx> for JSBiomeFormatter<'ctx> {
    /// Format the JS contents using the Biome formatter, by parsing them with
    /// `biome_js_parser` and formatting the tree with `biome_js_formatter`, or
    /// return `None` if they don't parse.
    fn format(&self, contents: &str) -> FmtResult<Option<String>> {
        // @@Todo: consider using `parse_js_with_cache` here, and store the
        // cache within our caching system.
        let parsed = js_parser::parse_script(contents, JsParserOptions::default());

        if parsed.has_errors() {
            return Ok(None);
        }

        let language = LanguageType::Js;
        let options = self.options.js.clone();

        match js_formatter::format_node(options, &parsed.syntax(), vec![]) {
            Ok(formatted) => Ok(Some(formatted.print().unwrap().into_code())),
            Err(_) => Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language })),
        }
    }

    /// Returns the terminal state of the JS formatter.
    ///
    /// Since a JS block is a terminal "node" in the context of a template i.e.
    /// there may not be any other embedded languages within the JS block,
    /// we can safely assume that the terminal state is `Js`, at the same
    /// indent.
    fn into_state(self) -> TerminalState {
        TerminalState { language: LanguageType::Js, ..self.context.state }
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
        HTMLBiomeFormatter {
            context,
            options: self.options.clone(),
            cache: self.html_cache.clone(),
            state: context.state,
        }
    }

    fn css_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::CSSEngine<'ctx> {
        CSSBiomeFormatter { context, options: self.options.clone() }
    }

    fn js_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::JSEngine<'ctx> {
        JSBiomeFormatter { context, options: self.options.clone() }
    }
}
