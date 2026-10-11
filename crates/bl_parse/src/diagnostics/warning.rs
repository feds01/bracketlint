//! Any warning that the parser can emit and report.

use bl_ast::{ForeignSyntax, Span};
use bl_reporting::{ReportBuilder, Reports};

#[derive(Debug, Clone, Copy)]
pub struct ParseWarning {
    kind: ParseWarningKind,
    span: Span,
}

#[derive(Debug, Clone, Copy)]
pub enum ParseWarningKind {
    /// A block label is a string literal instead of being an identifier.
    ///
    /// Whilst this is valid in some dialects, it is not valid in others.
    BlockLabelIsStringLiteral,

    /// A tag that is named as another dialect names one of its tags, e.g.
    /// Liquid's `elsif` in Jinja, which spells it `elif`. It is parsed as the
    /// dialect reads it: as a tag that bracketlint doesn't know, or as a raw
    /// block for `raw` in Django and Twig.
    ForeignTag(ForeignSyntax),

    /// A `{# #}` in a dialect without such comments, where it is text, see
    /// [Syntax::HashComments](bl_ast::Syntax::HashComments).
    HashComment(ForeignSyntax),
}

impl ParseWarning {
    pub fn new(kind: ParseWarningKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl From<ParseWarning> for Reports {
    fn from(warning: ParseWarning) -> Self {
        let ParseWarning { kind, span } = warning;

        let mut reporter = ReportBuilder::default();
        let report = reporter.warning();

        match kind {
            ParseWarningKind::BlockLabelIsStringLiteral => {
                report
                    .title("block label is a string literal")
                    .add_labelled_span(span, "block label is a string literal");
            }
            ParseWarningKind::ForeignTag(foreign) => {
                let tag = foreign.syntax.tag_name().unwrap_or_default();
                report
                    .title(format!("`{{% {tag} %}}` isn't a tag in {}", foreign.dialect.title()))
                    .add_span(span)
                    .add_help(foreign.help());
            }
            ParseWarningKind::HashComment(foreign) => {
                report
                    .title(format!("`{{# #}}` isn't a comment in {}", foreign.dialect.title()))
                    .add_span(span)
                    .add_help(foreign.help());
            }
        }

        reporter.into_reports()
    }
}
