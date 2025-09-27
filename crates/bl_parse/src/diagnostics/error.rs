//! Any parser errors that the parser can emit and report.

use bl_ast::Span;
use bl_lexer::token::TokenKind;
use bl_reporting::{ReportBuilder, Reports, help};
use derive_more::Constructor;

use super::expected::ExpectedItem;

/// A [ParseError] represents possible errors that occur when transforming the
/// token stream into the AST.
#[derive(Clone, Copy, Debug, Constructor)]
pub struct ParseError {
    /// The kind of error that was encountered.
    kind: ParseErrorKind,

    /// [Span] of where the error references to.
    span: Span,

    /// An optional vector of tokens that are expected to circumvent the error.
    expected: ExpectedItem,

    /// An optional token in question that was received byt shouldn't of been
    received: Option<TokenKind>,
}

#[derive(Clone, Copy, Debug)]
pub enum ParseErrorKind {
    /// Generic error specifying an expected token atom.
    UnExpected,

    /// Expected a top level statement, either being some block or
    /// just text. This is effectively an invariant as tokens at the
    /// top level can only be [TokenKind::Text] or delimited [TokenKind::Tree]s.
    Statement,

    /// Expected a statement to be a tag, or the block has tokens that begin
    /// a tag, e.g.
    ///
    /// ```django
    /// {% if user.is_active %}
    /// ```
    Tag,

    /// For tags that expect a termination, e.g. `{% endif %}`, this error
    /// represents the case where the termination is missing, e.g.
    /// ```ignore
    /// {% if user.is_active %}
    ///   ...
    ///
    /// <EOF>
    /// ```
    UnclosedTag,
}

impl From<ParseError> for Reports {
    fn from(err: ParseError) -> Self {
        let expected = err.expected;

        // Default label used when marking where the error occurred
        let span_label = "".to_string();

        // We can have multiple notes describing what could be done about the error.
        let mut help_notes = vec![];

        let mut base_message = match &err.kind {
            ParseErrorKind::UnExpected => match &err.received {
                Some(kind) => format!("unexpectedly encountered {}", kind.as_error_string()),
                None => "unexpectedly reached the end of input".to_string(),
            },
            ParseErrorKind::Statement => "expected a statement".to_string(),
            ParseErrorKind::Tag => "expected a tag".to_string(),
            ParseErrorKind::UnclosedTag => "expected a closing tag".to_string(),
        };

        // `ParseErrorKind::Expected` format the error message in their own way,
        // whereas all the other error types follow a conformed order to
        // formatting expected tokens.
        if !matches!(&err.kind, ParseErrorKind::UnExpected)
            && let Some(kind) = err.received
        {
            let atom_msg = format!(", however received {}", kind.as_error_string());
            base_message.push_str(&atom_msg);
        }

        // If the generated error has suggested tokens that aren't empty.
        if !expected.is_empty() {
            help_notes.push(help!("expected {expected}"));
        }

        // Now actually build the report
        let mut reporter = ReportBuilder::default();
        let report = reporter.error();
        report.title(base_message).add_labelled_span(err.span, span_label);

        // Add the `help` messages to the report
        for note in help_notes {
            report.add_element(note);
        }

        reporter.into_reports()
    }
}
