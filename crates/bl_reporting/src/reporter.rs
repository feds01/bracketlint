//! A diagnostic reporter for the Hash compiler.
//!
//! Has a fluent API for creating reports in a declarative way.
use std::fmt;

use annotate_snippets::{Level, Renderer, Snippet};
use bl_ast::HasSource;
use derive_more::Constructor;

use crate::{Report, ReportCodeBlock, ReportElement, ReportKind, Reports};

#[derive(Debug, Constructor, Default)]
pub struct ReportBuilder {
    reports: Reports,
}

impl ReportBuilder {
    /// Add a report to the builder.
    pub fn report(&mut self, kind: ReportKind) -> &mut Report {
        let mut report = Report::new();
        report.kind(kind);
        self.reports.push(report);
        self.reports.last_mut().unwrap()
    }

    /// Add an error report to the builder.
    pub fn error(&mut self) -> &mut Report {
        self.report(ReportKind::Error)
    }

    /// Add an info report to the builder.
    pub fn info(&mut self) -> &mut Report {
        self.report(ReportKind::Info)
    }

    /// Add a warning report to the builder.
    pub fn warning(&mut self) -> &mut Report {
        self.report(ReportKind::Warning)
    }

    /// Add an internal report to the builder.
    pub fn internal(&mut self) -> &mut Report {
        self.report(ReportKind::Internal)
    }

    /// Consume the [`Reporter`], producing a [`Vec<Report>`].
    pub fn into_reports(self) -> Reports {
        self.reports
    }
}

/// Facilitates the creation of lists of [Report]s in a declarative way.
#[derive(Debug)]
pub struct Reporter<'a, S: HasSource> {
    renderer: Option<&'a Renderer>,
    reports: Reports,
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
