use bl_ast::{HasSource, SourceId, Span};
use bl_utils::stream_less_writeln;

use crate::{ReportBuilder, Reporter};

/// A guard on when to print a particular message using `note_on_span`
/// and `panic_on_span` family functions.
#[derive(Clone, Copy)]
pub enum SpanGuard {
    /// No guard, we're always printing for any given module.
    Always,

    /// Guarded by a specific module.
    Module(SourceId),
}

pub struct InlineSnippet<'a, S: HasSource> {
    pub sources: &'a S,
    pub span: Span,
}

impl<'a, S: HasSource> InlineSnippet<'a, S> {
    pub fn new(sources: &'a S, span: impl Into<Span>) -> Self {
        Self { sources, span: span.into() }
    }
}

/// This macro will produce a [crate::report::Report] and then print it to the
/// standard output, this does not panic, it is intended as a debugging utility
/// to quickly print the `span` of something and the `message` associated with
/// it.
#[track_caller]
pub fn guarded_note_on_span<S: HasSource>(
    snippet: InlineSnippet<S>,
    message: impl ToString,
    guard: SpanGuard,
) {
    let InlineSnippet { sources, span } = snippet;

    let should_execute = match (guard, span.id) {
        (SpanGuard::Always, _) => true,
        (SpanGuard::Module(id), source) if source == id => true,
        _ => false,
    };

    // Don't report if the span guard isn't met.
    if !should_execute {
        return;
    }

    let mut builder = ReportBuilder::default();
    builder
        .info()
        .title(message)
        .add_labelled_span(span, "here")
        .add_note(format!("invoked at {}", ::core::panic::Location::caller()));

    stream_less_writeln!("{}", Reporter::new(sources, builder.into_reports()));
}

/// This macro will produce a [crate::report::Report] and then print it to the
/// standard output, this does not panic, it is intended as a debugging utility
/// for use when debugging the compiler.
///
/// ```ignore
/// // Don't print on `prelude` module.
/// note_on_span(item.span(), "compiling `item`");
///
/// // Always print.
/// note_on_span(item.span(), "compiling `item`");
/// ```
///
/// If you want to only print in certain conditions, i.e. only the entry point,
/// then you can use [guarded_note_on_span] instead.
pub fn note_on_span<S: HasSource>(snippet: InlineSnippet<S>, message: impl ToString) {
    guarded_note_on_span(snippet, message, SpanGuard::Always);
}
