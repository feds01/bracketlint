//! Contains all of the parsing logic for the `bl` project.

use bl_arena::Arena;
use bl_ast::{AstNode, AstNodeRef, AstVisitor, Dialect, Document, SourceId, Span, TempSourceMap};
use bl_ast_utils::{AstTreePrinter, TreeWriter, TreeWriterConfig};
use bl_lexer::{Lexer, LexerMetadata, token::Token};
use bl_reporting::{
    DiagnosticsMut, Report, ReportElement, Reports,
    inline::{InlineSnippet, note_on_span},
    note,
};
use bl_workspace::{Member, settings::DialectSource};
use diagnostics::ParserDiagnostics;
use parser::Parser;

mod diagnostics;
mod parser;

/// The options for the parsing operation.
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    /// Whether to recover from a parsing error, or terminate immediately
    /// upon encountering an error.
    pub recovery: bool,

    /// The dialect to parse the module in.
    pub dialect: Dialect,
}

impl ParseOptions {
    /// Create a new set of parse options with the given recovery setting.
    pub fn new(recovery: bool, dialect: Dialect) -> Self {
        Self { recovery, dialect }
    }
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { recovery: true, dialect: Dialect::default() }
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
    /// Create a new parse query with the given source ID and source, parsed in
    /// the member's dialect.
    pub fn new(id: SourceId, member: &'a Member) -> Self {
        Self { id, member, options: ParseOptions::new(true, member.dialect) }
    }

    /// Create a new parse query with the given source ID, source, and options.
    /// This is useful for when you want to customise the parsing operation.
    /// For example, you may want to disable error recovery.
    pub fn with_options(id: SourceId, member: &'a Member, options: ParseOptions) -> Self {
        Self { id, member, options }
    }
}

/// A structure that represents the result of the parsing operation.
pub struct ParseQueryResult<'a> {
    /// The resultant parsed module, in the arena that it was parsed into.
    ///
    /// If the node is `None`, then an unrecoverable error occurred during the
    /// parsing or lexing of a module.
    pub node: Option<AstNode<'a, Document<'a>>>,

    /// The parser may still produce diagnostics for this module, and so we
    /// want to propagate this
    pub diagnostics: Vec<Report>,
}

/// An entry point for the general framework to parse a module, into a tree in
/// `arena`.
pub fn parse_source<'a>(query: ParseQuery, arena: &'a Arena) -> ParseQueryResult<'a> {
    // let mut timings = StageMetrics::default();
    let ParseQuery { id, member, options } = query;

    let spanned = member.spanned();

    // Lex the contents of the module or interactive block
    let LexerMetadata { tokens, mut diagnostics } =
        Lexer::new(spanned, id, member.dialect).tokenise();
    let has_lexer_errors = diagnostics.has_errors();
    let mut reports = diagnostics.into_reports(Reports::from, Reports::from);

    // @@Todo: Make this a query instead so that we can call it as a debugging
    // tool.
    //
    // Print the tokens that we produce.
    //
    // emit_tokens(id, member, &tokens);

    // Check if the lexer has errors...
    if has_lexer_errors {
        note_dialect(&mut reports, member);
        return ParseQueryResult { node: None, diagnostics: reports };
    }

    // Create a new import resolver in the event of more modules that
    // are encountered whilst parsing this module.
    let mut diagnostics = ParserDiagnostics::new();
    let mut parser = Parser::new(id, arena, spanned, &tokens, &mut diagnostics, options);

    // Perform the parsing operation now... and send the result through the
    // message queue, regardless of it being an error or not.
    let node = parser.parse_document();

    reports.extend(diagnostics.into_reports(Reports::from, Reports::from));
    note_dialect(&mut reports, member);
    ParseQueryResult { node: Some(node), diagnostics: reports }
}

/// Add a note to each of `reports` that says which dialect `member` is parsed
/// in, and how it is picked, before the other notes of the report. A template
/// can read as another dialect's, so a diagnostic might only make sense in a
/// dialect that it isn't parsed in.
fn note_dialect(reports: &mut Reports, member: &Member) {
    let dialect = member.dialect;
    let source = match (member.dialect_source, dialect.extension()) {
        (DialectSource::Flag, _) => format!("from `--dialect {}`", dialect.name()),
        (DialectSource::Extension, Some(extension)) => format!("from its `.{extension}` extension"),
        _ => "the default, since neither `--dialect` nor its extension names a dialect".to_string(),
    };
    let message = format!("this file is parsed as {}, {source}", dialect.title());

    for report in reports {
        let notes =
            report.contents.iter().position(|element| matches!(element, ReportElement::Note(_)));
        report.contents.insert(notes.unwrap_or(report.contents.len()), note!("{message}"));
    }
}

pub fn emit_source_tree(member: &Member, document: AstNodeRef<Document>) {
    let spanned = member.spanned();
    let tree = AstTreePrinter::new(spanned).visit_document(document).unwrap();
    let config = TreeWriterConfig::unicode();
    log::info!(
        "parsed module '{}' as {}:\n{}",
        spanned.path.display(),
        member.dialect,
        TreeWriter::new_with_config(&tree, config)
    );
}

pub fn emit_tokens(id: SourceId, member: &Member, tokens: &[Token]) {
    let temp_map = TempSourceMap::from(member.with_id(id));
    for token in tokens {
        let snippet = InlineSnippet::new(&temp_map, Span::new(token.span, id));
        note_on_span(snippet, format!("token: {}", token.kind));
    }
}
