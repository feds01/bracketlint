//! Definitions for a diagnostic [Reporter] for the Bracketlint. [Reporter]
//! provides the necessary context for a [Report] to be rendered in a
//! human-readable format. This is done via the [annotate_snippets] crate and
//! its API.

use std::fmt;

use annotate_snippets::{Level, Renderer, Snippet};
use bl_ast::HasSource;

use crate::{Report, ReportCodeBlock, ReportElement, Reports};

/// Facilitates the creation of lists of [Report]s in a declarative way.
#[derive(Debug)]
pub struct Reporter<'a, S: HasSource> {
    /// The renderer that the reporter will use to render the reports.
    renderer: Option<&'a Renderer>,

    /// The list of reports that the reporter will render.
    reports: Reports,

    /// The source map that the reporter will use to access source information.
    sources: &'a S,
}

/// The default renderer for the reporter.
const DEFAULT_RENDERER: Renderer = Renderer::styled();

impl<'a, S: HasSource> Reporter<'a, S> {
    pub fn new(sources: &'a S, reports: Vec<Report>) -> Self {
        Self { reports, renderer: None, sources }
    }

    pub fn with_renderer(&mut self, renderer: &'a Renderer) -> &mut Self {
        self.renderer = Some(renderer);
        self
    }
}

impl<S: HasSource> fmt::Display for Reporter<'_, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let renderer = self.renderer.unwrap_or(&DEFAULT_RENDERER);

        for report in self.reports.iter() {
            let level = Level::from(report.kind);
            let mut message = level.title(&report.title);

            // If we have an associated code, we can add it to the message.
            if let Some(ref code) = report.error_code {
                message = message.id(code);
            }

            for element in &report.contents {
                match element {
                    ReportElement::CodeBlock(ReportCodeBlock { notes }) => {
                        for (span, note) in notes {
                            message = message.snippet(
                                Snippet::source(self.sources.contents(span.id))
                                    .origin(self.sources.path(span.id))
                                    .fold(true)
                                    .annotation(
                                        level
                                            .span(span.range.start()..(span.range.end() + 1))
                                            .label(note.as_str()),
                                    ),
                            );
                        }
                    }
                    ReportElement::Note(report_note) => {
                        message = message.footer(Level::Note.title(&report_note.message));
                    }
                }
            }

            writeln!(f, "{}", renderer.render(message))?;
        }

        Ok(())
    }
}
