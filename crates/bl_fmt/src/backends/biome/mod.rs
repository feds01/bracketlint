//! The Biome backend, which utilises the `biome` crate to parse and format
//! code.

use std::{cell::RefCell, collections::HashMap, ops::Range, rc::Rc};

use biome_css_formatter::{self as css_formatter, context::CssFormatOptions};
use biome_css_parser::{self as css_parser, CssOffsetParse, CssParserOptions};
use biome_formatter::{
    FormatError, IndentStyle, SourceMapGeneration, SourceMarker, prelude::Document,
};
use biome_html_formatter::{HtmlFormatOptions, format_node};
use biome_html_parser::{HtmlParse, HtmlParserOptions, parse_html_with_cache};
use biome_html_syntax::{
    AnyHtmlContent, AnyHtmlElement, HtmlElement, HtmlElementList, HtmlSyntaxKind, HtmlSyntaxNode,
    ScriptType, T, TextRange, TextSize, VOID_ELEMENTS,
};
use biome_js_formatter::{self as js_formatter, context::JsFormatOptions};
use biome_js_parser::{self as js_parser, JsOffsetParse, JsParserOptions};
use biome_languages::{CssFileSource, HtmlFileSource, JsFileSource};
use biome_rowan::{AstNode, AstNodeList, Direction, NodeCache, SyntaxKind, WalkEvent};
use bl_ast::ByteRange;

use crate::{
    adapters::{
        ExternalLanguagesEngineAdaptor, Formatted, FormatterContext, HasCSSParsing, HasHTMLParsing,
        HasJSParsing, HtmlPart, LanguageType, TerminalState, Unparsed,
    },
    diagnostics::{FmtError, FmtErrorKind, FmtResult},
    dialects::html::is_verbatim,
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

    /// The code of each [Body] in the piece that has been parsed, by where it
    /// starts in the piece. The piece is parsed again in its parts, each of
    /// which finds its bodies again, but the code of each is parsed once.
    codes: RefCell<HashMap<usize, Rc<ParsedCode>>>,
}

impl HTMLBiomeFormatter {
    fn parse(&self, contents: &str) -> HtmlParse {
        let options = HtmlParserOptions::from(&HtmlFileSource::html());
        parse_html_with_cache(contents, &mut self.cache.borrow_mut(), options)
    }

    /// The code of `body` in `contents`, which starts at `at` in the piece,
    /// parsed the first time that it is asked for.
    fn code(&self, contents: &str, at: usize, body: &Body) -> Rc<ParsedCode> {
        let start = at + body.content.start;
        if let Some(code) = self.codes.borrow().get(&start) {
            return code.clone();
        }

        let code = &contents[body.content.clone()];
        let offset = TextSize::from(start as u32);
        let parsed = Rc::new(match body.language {
            Code::Js(source) => {
                let options = JsParserOptions::default();
                ParsedCode::Js(
                    js_parser::parse_js_with_offset(code, offset, source, options),
                    source,
                )
            }
            Code::Css => ParsedCode::Css(css_parser::parse_css_with_offset(
                code,
                CssFileSource::css(),
                offset,
                css_parser_options(),
            )),
        });

        self.codes.borrow_mut().insert(start, parsed.clone());
        parsed
    }

    /// The part of the HTML at `range` in `contents`, which leaves the
    /// elements `opens` open, if it isn't empty. It is formatted, unless
    /// `broken` holds a tag in it, see [Scan::bogus], or it is only whitespace,
    /// which is laid out with what comes next. It is parsed on its own, unless
    /// it is the whole of `contents`, whose parse and scan are `scanned`.
    /// `contents` start at `at` in the piece.
    fn html_part(
        &self,
        (contents, at): (&str, usize),
        range: Range<usize>,
        broken: bool,
        opens: Vec<String>,
        scanned: Option<(&HtmlParse, &Scan)>,
    ) -> FmtResult<Option<HtmlPart>> {
        let Some(byte_range) = byte_range(&range) else {
            return Ok(None);
        };

        let (html, at) = (&contents[range.clone()], at + range.start);
        let formatted = if html.trim().is_empty() {
            Some(Formatted::default())
        } else if broken {
            None
        } else if let Some((parsed, scan)) = scanned {
            self.format_parsed((html, at), parsed, &scan.bodies)?
        } else {
            let parsed = self.parse(html);
            let scan = Scan::new(&parsed.syntax(), html.len());
            match scan.has_broken_tag() {
                true => None,
                false => self.format_parsed((html, at), &parsed, &scan.bodies)?,
            }
        };

        Ok(Some(HtmlPart::Html { range: byte_range, formatted, opens }))
    }

    /// Format `parsed`, the HTML `contents`, which start at `at` in the piece
    /// and have no tag that Biome can't parse, or return `None` if Biome still
    /// can't format it. The code of each of its `bodies`, which parses, is
    /// formatted as JavaScript or CSS, as Biome formats it in an HTML file.
    fn format_parsed(
        &self,
        (contents, at): (&str, usize),
        parsed: &HtmlParse,
        bodies: &[Body],
    ) -> FmtResult<Option<Formatted>> {
        let language = LanguageType::Html;
        let nodes = bodies.iter().map(|body| body.node).collect();
        match format_node(self.options.html.clone(), &parsed.syntax(), nodes) {
            // The source map only marks where each text starts and ends in the
            // code, which tells the lines that Biome keeps as they are written.
            Ok(mut formatted) => {
                formatted.format_embedded(|node| {
                    let body = bodies.iter().find(|body| body.node == node)?;
                    // The code is formatted once, so its parse is let go of then.
                    let code = self.code(contents, at, body);
                    self.codes.borrow_mut().remove(&(at + body.content.start));
                    code.format(&self.options)
                });
                formatted.propagate_expand();

                let printed = formatted.print_with_indent(0, SourceMapGeneration::Enabled).unwrap();
                let kept_lines = kept_lines(printed.as_code(), printed.sourcemap());
                Ok(Some(Formatted { code: printed.into_code(), kept_lines }))
            }
            // A tag without its `>`, e.g. `<div ` before `{{ attributes }}`,
            // has no node that Biome can format.
            Err(FormatError::SyntaxError) => Ok(None),
            Err(_) => Err(FmtError::new(FmtErrorKind::ExternalLanguageFormatError { language })),
        }
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
    /// [Scan::bogus]. The code of a `<script>` or `<style>` that no template
    /// tag splits is formatted with the HTML around it, see [Body].
    fn format(&mut self, contents: &str) -> FmtResult<Vec<HtmlPart>> {
        self.parts(contents, 0)
    }
}

impl HTMLBiomeFormatter {
    /// The parts of `contents`, which start at `at` in the piece, see
    /// [HasHTMLParsing::format].
    fn parts(&self, contents: &str, at: usize) -> FmtResult<Vec<HtmlPart>> {
        // A piece of only whitespace, e.g. between two tags, is laid out with
        // what comes next, so there's nothing to parse.
        if contents.trim().is_empty() {
            return Ok(self
                .html_part((contents, at), 0..contents.len(), false, vec![], None)?
                .into_iter()
                .collect());
        }

        let parsed = self.parse(contents);
        let html = parsed.tree().html();
        let mut scan = Scan::new(&parsed.syntax(), contents.len());

        // The code of a `<script>` or `<style>` that no tag splits is formatted
        // with the HTML around it, see [Body], unless it doesn't parse. Then
        // it is kept as it is written, as that of one that a tag splits is,
        // and reported. The scan finds the bodies before the first of the
        // other verbatim elements, so one that doesn't parse comes first.
        let mut unparsed = None;
        if let Some((content, error)) = scan.bodies.iter().find_map(|body| {
            Some((body.content.clone(), self.code(contents, at, body).unparsed(body)?))
        }) {
            scan.verbatim = Some(content);
            unparsed = Some(error);
        }

        // The content of a `<pre>`, `<textarea>`, or a `<script>` or `<style>`
        // that Biome doesn't format, is its own part, which is kept exactly as
        // it is written. Biome would print it as it is written too, but among
        // the HTML that it indents, which the formatter would then indent
        // again.
        if let Some(content) = scan.verbatim {
            let mut parts = self.parts(&contents[..content.start], at)?;
            parts.extend(byte_range(&content).map(|range| HtmlPart::Verbatim { range, unparsed }));

            let rest = self.parts(&contents[content.end..], at + content.end)?;
            parts.extend(rest.into_iter().map(|part| part.shift(content.end)));
            return Ok(parts);
        }

        // A part that holds a tag that a template tag cuts is kept as it is
        // written, see [Scan::bogus], so the element of a body that is outside
        // of such a tag is its own part, which is formatted.
        if let Some(body) = scan.bodies.first()
            && scan.broken_outside(&body.element)
        {
            let element = body.element.clone();
            let mut parts = self.parts(&contents[..element.start], at)?;

            let whole = self.parts(&contents[element.clone()], at + element.start)?;
            parts.extend(whole.into_iter().map(|part| part.shift(element.start)));

            let rest = self.parts(&contents[element.end..], at + element.end)?;
            parts.extend(rest.into_iter().map(|part| part.shift(element.end)));
            return Ok(parts);
        }

        let closes = std::mem::take(&mut scan.closes);
        if closes.is_empty() {
            let (all, broken) = (0..contents.len(), scan.has_broken_tag());
            let scanned = Some((&parsed, &scan));
            let part = self.html_part((contents, at), all, broken, open_elements(html), scanned)?;
            return Ok(part.into_iter().collect());
        }

        let mut parts = vec![];
        let mut start = 0;
        for (range, name) in closes {
            // What comes before the closing tag of a `<pre>`, `<textarea>`,
            // `<script>` or `<style>` is its content, which Biome would lay out
            // as other HTML, since it doesn't see its opening tag.
            let before = start..range.start;
            if is_verbatim(&name) {
                let verbatim = |range| HtmlPart::Verbatim { range, unparsed: None };
                parts.extend(byte_range(&before).map(verbatim));
            } else {
                // A part before a closing tag closes the elements it opens.
                let broken = scan.broken_in(&before);
                parts.extend(self.html_part((contents, at), before, broken, vec![], None)?);
            }

            start = range.end;
            parts.extend(byte_range(&range).map(|range| HtmlPart::Close { range, name }));
        }

        // The part at the end of the piece is the end of its list of elements,
        // so it leaves open the elements that the piece does.
        let rest = start..contents.len();
        let broken = scan.broken_in(&rest);
        parts.extend(self.html_part((contents, at), rest, broken, open_elements(html), None)?);
        Ok(parts)
    }
}

/// The lines of `code`, which Biome printed with the source `markers`, that
/// it keeps as they are written, by their index: each line after the first of
/// a text that spans several lines, e.g. a comment or an attribute value.
///
/// Biome marks where each text that it prints starts and ends, and prints its
/// own line breaks, indents and spaces between them. So a line break between
/// two markers with more than whitespace between them is one in a text.
fn kept_lines(code: &str, markers: &[SourceMarker]) -> Vec<usize> {
    let marks: Vec<usize> = markers.iter().map(|marker| marker.dest.into()).collect();
    let in_text = |newline: usize| {
        let next = marks.partition_point(|&mark| mark <= newline);
        match (next.checked_sub(1).map(|previous| marks[previous]), marks.get(next)) {
            (Some(start), Some(&end)) => !code[start..end].trim().is_empty(),
            _ => false,
        }
    };

    let newlines = code.match_indices('\n').map(|(newline, _)| newline);
    newlines
        .enumerate()
        .filter(|&(_, newline)| in_text(newline))
        .map(|(line, _)| line + 1)
        .collect()
}

/// The [ByteRange] of `range`, unless it is empty.
fn byte_range(range: &Range<usize>) -> Option<ByteRange> {
    (!range.is_empty()).then(|| ByteRange::new(range.start, range.end - 1))
}

/// What the formatter needs of the tree of a piece of HTML, found in one walk
/// over it, as the walk allocates each node.
struct Scan {
    /// The content of the first of the verbatim elements in the piece, see
    /// [verbatim_content], other than the bodies.
    verbatim: Option<Range<usize>>,

    /// The code of each `<script>` and `<style>` that Biome formats, see
    /// [Body], before the first of the other verbatim elements.
    bodies: Vec<Body>,

    /// The closing tags at the top of the piece, of elements that open before
    /// it, by their range and name.
    closes: Vec<(Range<usize>, String)>,

    /// Where each tag that Biome couldn't parse starts, see [Scan::bogus].
    broken: Vec<usize>,
}

impl Scan {
    fn new(root: &HtmlSyntaxNode, len: usize) -> Self {
        let mut scan = Scan { verbatim: None, bodies: vec![], closes: vec![], broken: vec![] };
        let mut walk = root.preorder();

        while let Some(event) = walk.next() {
            let WalkEvent::Enter(node) = event else {
                continue;
            };

            if node.kind().is_bogus() {
                scan.bogus(&node);
                walk.skip_subtree();
            } else if scan.verbatim.is_none()
                && let Some(element) = HtmlElement::cast(node)
            {
                match Body::of(&element, len) {
                    Some(body) => scan.bodies.push(body),
                    None => scan.verbatim = verbatim_content(&element, len),
                }
            }
        }

        scan
    }

    /// Note a `node` that Biome couldn't parse, which is a closing tag that
    /// ends a part, a broken tag, or neither.
    ///
    /// A closing tag whose element opens before the piece, e.g. the `</li>` in
    /// `{% endif %}</li>`, has no element to close, so Biome parses it into a
    /// bogus node. One at the top of the piece is where a part ends, see
    /// [HtmlPart::Close]. One inside an element is printed as any other
    /// closing tag, unless it spans several lines, e.g. `</li\n>`, or it closes
    /// one of the
    /// [VERBATIM_ELEMENTS](crate::dialects::html::VERBATIM_ELEMENTS), whose
    /// content Biome would then lay out as other HTML.
    ///
    /// Any other is a broken tag, e.g. `<li class="` that a template tag cuts.
    /// Biome prints it as it is written, among the HTML that it lays out, so
    /// that formatting the piece again lays it out differently. For one, the
    /// other lines of such a tag keep the indentation of the source, which the
    /// formatter would indent again each run. What a broken tag holds doesn't
    /// matter, nor where it starts exactly, as the whitespace before it is in
    /// the same part.
    fn bogus(&mut self, node: &HtmlSyntaxNode) {
        let at_top = node
            .parent()
            .and_then(|list| list.parent())
            .is_some_and(|root| root.kind() == HtmlSyntaxKind::HTML_ROOT);

        match Bogus::of(node) {
            Bogus::Closing(name) if at_top => {
                let range = node.text_trimmed_range();
                self.closes.push((range.start().into()..range.end().into(), name));
            }
            Bogus::Closing(name)
                if !is_verbatim(&name) && !node.text_trimmed().contains_char('\n') => {}
            _ => self.broken.push(node.text_range_with_trivia().start().into()),
        }
    }

    /// Whether the piece has a tag that Biome couldn't parse.
    fn has_broken_tag(&self) -> bool {
        !self.broken.is_empty()
    }

    /// Whether a tag that Biome couldn't parse starts in `range` of the piece.
    fn broken_in(&self, range: &Range<usize>) -> bool {
        self.broken.iter().any(|start| range.contains(start))
    }

    /// Whether a tag that Biome couldn't parse starts outside of `range` of
    /// the piece.
    fn broken_outside(&self, range: &Range<usize>) -> bool {
        self.broken.iter().any(|start| !range.contains(start))
    }
}

/// What a bogus node of Biome's tree is, from its first tokens.
enum Bogus {
    /// The closing tag of the element `name`, e.g. `</li>`.
    Closing(String),

    /// A start tag that a template tag cuts, e.g. `<a href="`, of an element
    /// `name` that isn't void, i.e. one that has content and a closing tag,
    /// unlike `<img>`.
    Start(String),

    /// Anything else, e.g. the cut start tag of a void element.
    Other,
}

impl Bogus {
    fn of(node: &HtmlSyntaxNode) -> Self {
        let mut tokens = node.descendants_tokens(Direction::Next);
        let (Some(open), Some(next)) = (tokens.next(), tokens.next()) else {
            return Bogus::Other;
        };

        match next.kind() {
            _ if open.kind() != T![<] => Bogus::Other,
            T![/] => tokens
                .next()
                .map_or(Bogus::Other, |name| Bogus::Closing(name.text_trimmed().to_string())),
            kind if VOID_ELEMENTS.contains(kind) => Bogus::Other,
            _ => Bogus::Start(next.text_trimmed().to_string()),
        }
    }
}

/// The content of `element`, if it is one of the verbatim elements, see
/// [is_verbatim], and it isn't empty: from the end of its opening tag to the
/// start of its closing tag, or to the end of the piece, `len`, if it has
/// none. An opening tag that a template tag cuts before its `>`, e.g. `<pre `
/// in `<pre {% if x %}`, has no content in the piece, as the rest of it is the
/// rest of the tag.
fn verbatim_content(element: &HtmlElement, len: usize) -> Option<Range<usize>> {
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
}

/// The code of a `<script>` of JavaScript or a `<style>` of CSS that no
/// template tag splits, i.e. whose opening and closing tags are both in the
/// piece. Biome formats it with the HTML around it, as it does in an HTML
/// file, when it is given the range of its node. The code of a `<script>` of
/// another type, e.g. JSON or a template that a library renders, is kept as
/// it is written.
struct Body {
    /// The element, from the start of its opening tag to the end of its
    /// closing tag.
    element: Range<usize>,

    /// The content of the element, see [verbatim_content].
    content: Range<usize>,

    /// The range of the node of the code, by which Biome asks for it to be
    /// formatted.
    node: TextRange,

    /// The language of the code.
    language: Code,
}

/// The language of the code of a [Body].
#[derive(Clone, Copy)]
enum Code {
    /// JavaScript, as a classic script or as a module.
    Js(JsFileSource),

    /// CSS.
    Css,
}

impl Body {
    /// The code of `element`, in a piece of length `len`, if it is a
    /// `<script>` of JavaScript or a `<style>` of CSS, as Biome tells them,
    /// with its closing tag.
    fn of(element: &HtmlElement, len: usize) -> Option<Self> {
        element.closing_element()?;
        let language = match element.get_script_type() {
            Some(ScriptType::Classic) => Code::Js(JsFileSource::js_script()),
            Some(ScriptType::Module) => Code::Js(JsFileSource::js_module()),
            Some(_) => return None,
            // A `<style>` of another type, e.g. `text/less`, isn't CSS.
            None if element.is_supported_style_tag()
                && (element.find_attribute_by_name("type").is_none()
                    || element.has_attribute_with_value("type", "text/css")) =>
            {
                Code::Css
            }
            None => return None,
        };

        let code = element.children().iter().find_map(|child| match child {
            AnyHtmlElement::AnyHtmlContent(AnyHtmlContent::HtmlEmbeddedContent(code)) => Some(code),
            _ => None,
        })?;

        // Old pages wrap the code in an HTML comment, `<!-- ... -->`, to hide
        // it from browsers that don't know the element. It is still JavaScript
        // or CSS, but Biome can't parse it.
        if code.value_token().ok()?.text_trimmed().trim_start().starts_with("<!--") {
            return None;
        }

        Some(Body {
            element: element.syntax().text_trimmed_range().into(),
            content: verbatim_content(element, len)?,
            node: code.range(),
            language,
        })
    }
}

/// The code of a [Body], as Biome parsed it.
enum ParsedCode {
    Js(JsOffsetParse, JsFileSource),
    Css(CssOffsetParse),
}

impl ParsedCode {
    /// Where and why Biome first fails to parse the code of `body`, in its
    /// piece, if it does.
    fn unparsed(&self, body: &Body) -> Option<Unparsed> {
        let (language, diagnostics) = match self {
            ParsedCode::Js(parsed, _) => (LanguageType::Js, parsed.diagnostics()),
            ParsedCode::Css(parsed) => (LanguageType::Css, parsed.diagnostics()),
        };

        let diagnostic = diagnostics.first()?;
        let span =
            diagnostic.span().unwrap_or_default() + TextSize::from(body.content.start as u32);

        // An empty span, e.g. where the code ends without its last `}`, marks
        // the character there, which is the `<` of the closing tag at the end.
        let (start, end) = (usize::from(span.start()), usize::from(span.end()));
        let range = ByteRange::new(start, end.saturating_sub(1).max(start));
        Some(Unparsed { language, range, message: diagnostic.message.to_string() })
    }

    /// The code, formatted, or `None` if it doesn't parse, or if Biome can't
    /// format it, in which case Biome prints it as it is written.
    fn format(&self, options: &BiomeFormatterOptions) -> Option<Document> {
        match self {
            // The options are those of the code's source type, e.g. a module.
            ParsedCode::Js(parsed, source) if !parsed.has_errors() => {
                let options = JsFormatOptions::new(*source);
                let formatted = js_formatter::format_node_with_offset(options, &parsed.syntax());
                formatted.ok().map(|formatted| formatted.into_document())
            }
            ParsedCode::Css(parsed) if !parsed.has_errors() => {
                let options = options.css.clone();
                let formatted = css_formatter::format_node_with_offset(options, &parsed.syntax());
                formatted.ok().map(|formatted| formatted.into_document())
            }
            _ => None,
        }
    }
}

/// The options that CSS is parsed with: `//` comments and Grit's metavariables
/// are allowed.
fn css_parser_options() -> CssParserOptions {
    CssParserOptions::default().allow_wrong_line_comments().allow_metavariables()
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
                if let Bogus::Start(name) = Bogus::of(bogus.syntax()) {
                    opens.push(name);
                }

                break;
            }
            _ => break,
        }
    }

    opens
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
        let parsed = css_parser::parse_css(contents, CssFileSource::css(), css_parser_options());
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
        HTMLBiomeFormatter {
            options: self.options.clone(),
            cache: self.html_cache.clone(),
            codes: RefCell::default(),
        }
    }

    fn css_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::CSSEngine<'ctx> {
        CSSBiomeFormatter { context, options: self.options.clone() }
    }

    fn js_engine<'ctx>(&self, context: &'ctx FormatterContext<'ctx>) -> Self::JSEngine<'ctx> {
        JSBiomeFormatter { context, options: self.options.clone() }
    }
}
