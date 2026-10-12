//! Any warning that the parser can emit and report.

use bl_ast::{Dialect, ForeignSyntax, Keyword, Span};
use bl_reporting::{ReportBuilder, ReportKind, Reports};

use super::error::ParseError;

#[derive(Debug, Clone)]
pub struct ParseWarning {
    kind: ParseWarningKind,
    span: Span,
}

#[derive(Debug, Clone)]
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

    /// The markup of a tag or a `{{ }}` that doesn't parse, which is kept as it
    /// is written, see
    /// [Dialect::markup_can_be_lax](bl_ast::Dialect::markup_can_be_lax). It
    /// holds the error that parsing the markup reported, whose title, span
    /// and help the warning has.
    LaxMarkup(ParseError),

    /// Text after the name of an `{% else %}` or end tag, which `dialect`
    /// ignores, e.g. the `if b` in Liquid's `{% else if b %}`, see
    /// [Dialect::else_and_end_tags_ignore_markup].
    IgnoredMarkup {
        dialect: Dialect,

        /// The name of the tag, e.g. `else`.
        tag: Keyword,

        /// The text, as it is written.
        markup: Box<str>,

        /// Whether the tag is the `{% else %}` of an `if` block, which could
        /// be an `elif` clause instead.
        in_if: bool,
    },
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
            // The warning reads as the error would, with a note on why it
            // isn't one.
            ParseWarningKind::LaxMarkup(error) => {
                let mut reports = Reports::from(error);
                for report in &mut reports {
                    report.kind(ReportKind::Warning).add_note(
                        "Liquid's strict parser rejects this, and Jekyll reads it with the lax \
                         parser instead, so it is kept as it is written",
                    );
                }

                return reports;
            }
            ParseWarningKind::IgnoredMarkup { dialect, tag, markup, in_if } => {
                report
                    .title(format!("{} ignores `{markup}` after `{tag}`", dialect.title()))
                    .add_span(span);

                // An `if` after the `else` of an `if` block likely means a
                // branch with a condition, e.g. `{% else if b %}`.
                if in_if
                    && let Some(condition) = markup.strip_prefix("if")
                    && condition.starts_with(char::is_whitespace)
                {
                    let (elif, condition) = (dialect.elif_tag(), condition.trim_start());
                    report.add_help(format!(
                        "`{{% {elif} {condition} %}}` starts a branch with a condition"
                    ));
                }
            }
        }

        reporter.into_reports()
    }
}
