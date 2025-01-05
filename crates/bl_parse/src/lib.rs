//! Contains all of the parsing logic for the `bl` project.
#![allow(dead_code)]

use bl_ast::{AstNode, Document, LocalSpanMap, SourceId, SpanMap, SpannedSource};
use bl_lexer::{Lexer, LexerMetadata};
use bl_reporting::{DiagnosticsMut, Report, Reports};
use bl_workspace::Member;
use diagnostics::ParserDiagnostics;
use parser::Parser;

mod diagnostics;
mod parser;

/// The options for the parsing operation.
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    /// Whether to recover from a parsing error, or terminate immediately
    /// upon encountering an error.
    recovery: bool,
}

impl ParseOptions {
    /// Create a new set of parse options with the given recovery setting.
    pub fn new(recovery: bool) -> Self {
        Self { recovery }
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { recovery: true }
    }
}

/// A query structure that represents the input to the parsing operation.
pub struct ParseQuery<'a> {
    /// The source ID of the module that was parsed.
    id: SourceId,

    /// The contents of the module to parse.
    member: &'a Member,

    /// The options for the parsing operation.
    options: ParseOptions,
}

impl<'a> ParseQuery<'a> {
    /// Create a new parse query with the given source ID and source.
    pub fn new(id: SourceId, member: &'a Member) -> Self {
        Self { id, member, options: ParseOptions::default() }
    }

    /// Create a new parse query with the given source ID, source, and options.
    /// This is useful for when you want to customise the parsing operation.
    /// For example, you may want to disable error recovery.
    pub fn with_options(id: SourceId, member: &'a Member, options: ParseOptions) -> Self {
        Self { id, member, options }
    }
}

/// A structure that represents the result of the parsing operation.
pub struct ParseResult {
    /// The resultant parsed module.
    ///
    /// If the node is `None`, then an unrecoverable error occurred during the
    /// parsing or lexing of a module.
    pub node: Option<AstNode<Document>>,

    /// The parser may still produce diagnostics for this module, and so we
    /// want to propagate this
    pub diagnostics: Vec<Report>,
}

/// An entry point for the general framework to parse a module.
pub fn parse_source(query: ParseQuery) -> ParseResult {
    // let mut timings = StageMetrics::default();
    let ParseQuery { id, member, options } = query;

    let spanned = SpannedSource::from_string(&member.contents);

    // Lex the contents of the module or interactive block
    let LexerMetadata { tokens, mut diagnostics } = Lexer::new(spanned, id).tokenise();
    let mut spans = LocalSpanMap::with_capacity(tokens.len() * 2);

    // Print the tokens that we produce.
    // let temp_map = TempSourceMap::from((&member).with_id(id));
    // for token in &tokens {
    //     let snippet = InlineSnippet::new(&temp_map, Span::new(token.span, id));
    //     note_on_span(snippet, format!("token: {}", token.kind));
    // }

    // Check if the lexer has errors...
    if diagnostics.has_errors() {
        SpanMap::add_local_map(id, spans);
        return ParseResult {
            node: None,
            diagnostics: diagnostics.into_reports(Reports::from, Reports::from),
        };
    }

    // Create a new import resolver in the event of more modules that
    // are encountered whilst parsing this module.
    let mut diagnostics = ParserDiagnostics::new();
    let mut parser = Parser::new(spanned, &tokens, &mut diagnostics, &mut spans, options);

    // Perform the parsing operation now... and send the result through the
    // message queue, regardless of it being an error or not.
    let node = parser.parse_document();

    SpanMap::add_local_map(id, spans);
    ParseResult {
        node: Some(node),
        diagnostics: diagnostics.into_reports(Reports::from, Reports::from),
    }
}
