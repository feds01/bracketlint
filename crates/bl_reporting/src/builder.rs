//! Definition of the [ReportBuilder]. This provides a [builder style](https://rust-lang.github.io/api-guidelines/type-safety.html#builders-enable-construction-of-complex-values-c-builder)
//! accumulator for [Reports]s.

use derive_more::Constructor;

use crate::{Report, ReportKind, Reports};

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
