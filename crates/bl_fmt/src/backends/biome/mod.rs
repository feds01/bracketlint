//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use std::{cell::RefCell, ops::Range, rc::Rc};

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssParserOptions};
use biome_formatter::{FormatError, IndentStyle};
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::{HtmlParse, HtmlParserOptions, parse_html_with_cache};
use biome_html_syntax::{
    AnyHtmlElement, HtmlElement, HtmlElementList, HtmlSyntaxNode, T, VOID_ELEMENTS,
};
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsParserOptions};
use biome_languages::{CssFileSource, HtmlFileSource, JsFileSource};
use biome_rowan::{AstNode, AstNodeList, Direction, NodeCache, SyntaxKind};

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, FormatterContext, HasCSSParsing, HasHTMLParsing,
        HasJSParsing, HtmlPart, LanguageType, TerminalState, is_verbatim,
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

struct HTMLBiomeFormatter {
    /// The options for the formatter, encapsulating backend specific options.
    options: BiomeFormatterOptions,

    /// The cache of the nodes and tokens that the HTML parser makes.
    cache: Rc<RefCell<NodeCache>>,
}

impl HTMLBiomeFormatter {
    fn parse(&self, contents: &str) -> HtmlParse {
        let options = HtmlParserOptions::from(&HtmlFileSource::html());
        parse_html_with_cache(contents, &mut self.cache.borrow_mut(), options)
    }

    /// The part of HTML at `range` in its piece, which `parsed` is, with the
    /// elements it leaves open. It is kept as it is written if Biome can't
    /// format it.
    fn html_part(&self, range: Range<usize>, parsed: &HtmlParse) -> FmtResult<HtmlPart> {
        let root = parsed.syntax();
        let opens = open_elements(parsed.tree().html());

        if has_broken_tag(&root) {
            return Ok(HtmlPart::Html { range, formatted: None, opens });
        }

        let language = LanguageType::Html;
        let options = self.options.html.clone();

        let formatted = match format_node(options, &root, vec![]) {
            Ok(formatted) => formatted.print().unwrap().into_code(),
            // A tag without its `>`, e.g. `<div ` before `{{ attributes }}`,
            // has no node that Biome can format.
            Err(FormatError::SyntaxError) => {
                return Ok(HtmlPart::Html { range, formatted: None, opens });
            }
            Err(_) => {
                return Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language }));
            }
        };

        Ok(HtmlPart::Html { range, formatted: Some(formatted), opens })
    }
}

impl<'ctx> HasHTMLParsing<'ctx> for HTMLBiomeFormatter {
    /// Format the HTML contents using the Biome formatter, by parsing them
    /// with `biome_html_parser` and formatting the tree with
    /// `biome_html_formatter`.
    ///
    /// ##Note: The contents are a piece of the template between its tags,
    /// which often isn't valid HTML on its own. Elements that the piece opens
    /// without closing them are formatted all the same. The closing tags of
    /// elements that open before the piece are its own parts, so that each is
    /// indented by the elements that are still open. A part where a template
    /// tag cuts an HTML tag, e.g. `<li class="`, is kept as it is written, see
    /// [has_broken_tag].
    fn format(&mut self, contents: &str) -> FmtResult<Vec<HtmlPart>> {
        let parsed = self.parse(contents);

        // The content of a `<pre>`, `<textarea>`, `<script>` or `<style>` is
        // its own part, which is kept exactly as it is written. Biome would
        // print it as it is written too, but among the HTML that it indents,
        // which the formatter would then indent again.
        if let Some(content) = verbatim_content(&parsed.syntax(), contents.len()) {
            let mut parts = self.format(&contents[..content.start])?;
            parts.push(HtmlPart::Verbatim { range: content.clone() });

            let rest = self.format(&contents[content.end..])?;
            parts.extend(rest.into_iter().map(|part| part.shift(content.end)));
            return Ok(parts);
        }

        let closes = closing_tags(parsed.tree().html());
        if closes.is_empty() {
            return Ok(vec![self.html_part(0..contents.len(), &parsed)?]);
        }

        let mut parts = vec![];
        let mut start = 0;
        for (range, name) in closes {
            // What comes before the closing tag of a `<pre>`, `<textarea>`,
            // `<script>` or `<style>` is its content, which Biome would lay out
            // as other HTML, since it doesn't see its opening tag.
            let html = start..range.start;
            if is_verbatim(&name) {
                parts.push(HtmlPart::Verbatim { range: html });
            } else {
                parts.push(self.html_part(html.clone(), &self.parse(&contents[html]))?);
            }

            start = range.end;
            parts.push(HtmlPart::Close { range, name });
        }

        let rest = start..contents.len();
        parts.push(self.html_part(rest.clone(), &self.parse(&contents[rest]))?);
        Ok(parts)
    }
}

/// The content of the first of the
/// [VERBATIM_ELEMENTS](crate::adapters::VERBATIM_ELEMENTS) in `root`, which
/// isn't empty: from the end of its opening tag to the start of its closing
/// tag, or to the end of the piece, `len`, if it has none. An opening tag that
/// a template tag cuts before its `>`, e.g. `<pre ` in `<pre {% if x %}`, has
/// no content in the piece, as the rest of it is the rest of the tag.
fn verbatim_content(root: &HtmlSyntaxNode, len: usize) -> Option<Range<usize>> {
    let content = |element: HtmlElement| {
        let opening = element.opening_element().ok()?;
        let name = opening.tag_name()?;
        if !is_verbatim(name.text()) || opening.r_angle_token().is_err() {
            return None;
        }

        let start = opening.syntax().text_trimmed_range().end().into();
        let end = element
            .closing_element()
            .map_or(len, |closing| closing.syntax().text_trimmed_range().start().into());
        (start < end).then_some(start..end)
    };

    root.descendants().filter_map(HtmlElement::cast).find_map(content)
}

/// The name of the closing tag `node`, if it is one.
fn closing_tag_name(node: &HtmlSyntaxNode) -> Option<String> {
    let mut tokens = node.descendants_tokens(Direction::Next);
    let (open, slash, name) = (tokens.next()?, tokens.next()?, tokens.next()?);
    let is_closing_tag = open.kind() == T![<] && slash.kind() == T![/];
    is_closing_tag.then(|| name.text_trimmed().to_string())
}

/// The closing tags in `list` of elements that open before it, by their range
/// and name. Biome parses each into a bogus element, as it has no element to
/// close.
fn closing_tags(list: HtmlElementList) -> Vec<(Range<usize>, String)> {
    let closing_tag = |element: AnyHtmlElement| {
        let AnyHtmlElement::HtmlBogusElement(bogus) = element else {
            return None;
        };

        let range = bogus.syntax().text_trimmed_range();
        let name = closing_tag_name(bogus.syntax())?;
        Some((range.start().into()..range.end().into(), name))
    };

    list.iter().filter_map(closing_tag).collect()
}

/// The elements that `list` leaves open, outermost first. An element without
/// its closing tag holds the rest of the list, so it is the last element of
/// the list, and the elements open inside it are the last of its children.
fn open_elements(mut list: HtmlElementList) -> Vec<String> {
    let mut opens = vec![];

    loop {
        match list.iter().last() {
            Some(AnyHtmlElement::HtmlElement(element)) if element.closing_element().is_none() => {
                let Some(name) = element.opening_element().ok().and_then(|tag| tag.tag_name())
                else {
                    break;
                };

                opens.push(name.text().to_string());
                list = element.children();
            }
            // A start tag that a template tag cuts in a value, e.g. `<a href="`
            // before `{{ url }}`, is a bogus element, and its element opens
            // once a later piece ends the tag.
            Some(AnyHtmlElement::HtmlBogusElement(bogus)) => {
                opens.extend(cut_start_tag(bogus.syntax()));
                break;
            }
            _ => break,
        }
    }

    opens
}

/// The name of the element that `node` starts, if it is a start tag that a
/// template tag cuts, e.g. `<a href="`, of an element that isn't void, i.e.
/// one that has content and a closing tag, unlike `<img>`.
fn cut_start_tag(node: &HtmlSyntaxNode) -> Option<String> {
    let mut tokens = node.descendants_tokens(Direction::Next);
    let (open, name) = (tokens.next()?, tokens.next()?);
    let is_start_tag =
        open.kind() == T![<] && name.kind() != T![/] && !VOID_ELEMENTS.contains(name.kind());
    is_start_tag.then(|| name.text_trimmed().to_string())
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
/// closes one of the [VERBATIM_ELEMENTS](crate::adapters::VERBATIM_ELEMENTS),
/// whose content Biome would then lay out as other HTML, since it only knows it
/// from their opening tag.
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
            && !is_verbatim(name)
            && !node.text_trimmed().contains_char('\n')
    };

    root.descendants().any(|node| node.kind().is_bogus() && !is_closing_tag(&node))
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

    fn html_engine<'ctx>(&self, _: &'ctx FormatterContext<'ctx>) -> Self::HTMLEngine<'ctx> {
        HTMLBiomeFormatter { options: self.options.clone(), cache: self.html_cache.clone() }
    }

    fn css_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::CSSEngine<'ctx> {
        CSSBiomeFormatter { context, options: self.options.clone() }
    }

    fn js_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::JSEngine<'ctx> {
        JSBiomeFormatter { context, options: self.options.clone() }
    }
}
