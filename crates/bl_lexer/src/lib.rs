//! BL lexer implementation for the parser.

pub mod diagnostics;
pub mod token;

use bl_ast::{SourceId, SpannedSource};
use diagnostics::LexerDiagnostics;
use token::Token;

/// Metadata that the lexer produces once it finished processing the given
/// input.
pub struct LexerMetadata {
    /// Token tree store, essentially a collection of token trees that are
    /// produced when the lexer encounters bracketed token streams.
    pub tokens: Vec<Token>,

    /// Diagnostics produced by the lexer.
    pub diagnostics: LexerDiagnostics,
}

/// The lexer itself, which is responsible for converting a stream of characters
/// into a stream of tokens.
///
/// It is intended that a lexer is "consumed" after it has been used to produce
/// a stream of tokens. If the lexer encounters a fatal error, this is still
/// considered to be a successful operation, and the lexer will still produce a
/// stream of tokens that it was able to produce before the error occurred.
pub struct Lexer<'lex> {
    /// The source that the lexer is processing.
    pub source: SpannedSource<'lex>,

    /// The ID of the member that the lexer is processing, useful for error
    /// reporting.
    pub id: SourceId,

    /// Diagnostics that the lexer has produced.
    pub diagnostics: LexerDiagnostics,

    /// The tokens that the lexer has produced.
    pub tokens: Vec<Token>,
}

impl<'lex> Lexer<'lex> {
    pub fn new(source: SpannedSource<'lex>, id: SourceId) -> Self {
        Self { source, id, diagnostics: LexerDiagnostics::default(), tokens: Vec::new() }
    }

    /// Consume the lexer and produce a stream of tokens.
    pub fn tokenise(self) -> LexerMetadata {
        LexerMetadata { tokens: self.tokens, diagnostics: self.diagnostics }
    }
}
