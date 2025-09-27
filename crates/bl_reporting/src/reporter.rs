//! Definitions for a diagnostic [Reporter] for the Bracketlint. [Reporter]
//! provides the necessary context for a [Report] to be rendered in a
//! human-readable format. This is done via the [annotate_snippets] crate and
//! its API.

use std::fmt;

use annotate_snippets::{AnnotationKind, Element, Level, Renderer, Snippet};
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
            let mut message = level.primary_title(&report.title);

            // If we have an associated code, we can add it to the message.
            if let Some(ref code) = report.error_code {
                message = message.id(code);
            }

            let mut elements: Vec<Element<'_>> = vec![];

            for element in &report.contents {
                match element {
                    ReportElement::CodeBlock(ReportCodeBlock { notes }) => {
                        for (span, note) in notes {
                            elements.push(
                                Snippet::source(self.sources.contents(span.id))
                                    .path(self.sources.path(span.id))
                                    .fold(true)
                                    .annotation(
                                        AnnotationKind::Context
                                            .span(span.range.start()..(span.range.end() + 1))
                                            .label(note.as_str()),
                                    )
                                    .into(),
                            );
                        }
                    }
                    ReportElement::Note(report_note) => {
                        elements.push(Level::INFO.message(&report_note.message).into());
                    }
                }
            }

            let report = &[message.elements(elements)];

            writeln!(f, "{}", renderer.render(report))?;
        }

        Ok(())
    }
}
