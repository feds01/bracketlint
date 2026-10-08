//! Templates that the bracketlint benchmarks run on. The ones from files are
//! in `resources/`, see `resources/README.md` for where each came from.

use std::{fmt, path::PathBuf};

use bl_ast::{Dialect, SourceId};
use bl_parse::{ParseQuery, parse_source};
use bl_workspace::WorkspaceMembers;

/// A template that a benchmark lexes, parses or formats.
pub struct TestCase {
    /// The name that the benchmark reports the template under.
    pub name: &'static str,

    /// The dialect that the template is written in.
    pub dialect: Dialect,

    /// The contents of the template.
    pub contents: String,
}

impl TestCase {
    fn new(name: &'static str, dialect: Dialect, contents: impl Into<String>) -> Self {
        Self { name, dialect, contents: contents.into() }
    }

    /// Every template that the benchmarks run on.
    pub fn all() -> Vec<TestCase> {
        vec![
            Self::new(
                "django_technical_500",
                Dialect::Django,
                include_str!("../resources/django_technical_500.html"),
            ),
            Self::new(
                "liquid_minimal_mistakes_single",
                Dialect::Liquid,
                include_str!("../resources/minimal_mistakes_single.liquid"),
            ),
            Self::new(
                "django_shop",
                Dialect::Django,
                include_str!("../resources/django_shop.html"),
            ),
            Self::new(
                "liquid_shop",
                Dialect::Liquid,
                include_str!("../resources/liquid_shop.liquid"),
            ),
            Self::new("django_nested_blocks", Dialect::Django, nested_blocks(200)),
            Self::new("django_many_tags", Dialect::Django, many_tags(500)),
        ]
    }

    /// The templates that the formatter benchmarks run on, which are those
    /// that it formats without errors.
    ///
    /// ##Note: The formatter can't yet format the CSS in `django_technical_500`
    /// or some of the HTML in `liquid_minimal_mistakes_single`, and would only
    /// benchmark its error path on them.
    pub fn formattable() -> Vec<TestCase> {
        let unformattable = ["django_technical_500", "liquid_minimal_mistakes_single"];
        Self::all().into_iter().filter(|case| !unformattable.contains(&case.name)).collect()
    }

    /// Make the template the only member of a workspace, returning its
    /// [SourceId] and the members that it belongs to.
    pub fn to_member(&self) -> (SourceId, WorkspaceMembers) {
        let mut members = WorkspaceMembers::new();
        let path = PathBuf::from(self.name);
        let id = members.reserve_member(path, self.contents.clone(), self.dialect);
        (id, members)
    }

    /// Like [TestCase::to_member], but with the template parsed into the
    /// member's document, which is what the formatter reads.
    pub fn to_parsed_member(&self) -> (SourceId, WorkspaceMembers) {
        let (id, mut members) = self.to_member();
        let result = parse_source(ParseQuery::new(id, members.member(id)));
        members.member_mut(id).document = result.node;
        (id, members)
    }
}

impl fmt::Display for TestCase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// `depth` blocks, each nested in the one before it.
fn nested_blocks(depth: usize) -> String {
    let open = "{% if value %}<div>".repeat(depth);
    let close = "</div>{% endif %}".repeat(depth);
    format!("{open}{{{{ value }}}}{close}")
}

/// A loop over `lines` lines, each with a few short tags.
fn many_tags(lines: usize) -> String {
    let line = "<li class=\"{{ item.class }}\">{{ item.name|title }} \
                {% if item.active %}active{% endif %}</li>\n";
    format!("{{% for item in items %}}\n{}{{% endfor %}}\n", line.repeat(lines))
}

#[cfg(test)]
mod tests {
    use bl_fmt::{FormatQuery, FormatterOptions, fmt_module};
    use bl_parse::{ParseQuery, parse_source};

    use super::TestCase;

    /// A template that doesn't parse would only benchmark the error path, so
    /// make sure that each of them parses without errors.
    #[test]
    fn test_cases_parse_without_errors() {
        for case in TestCase::all() {
            let (id, members) = case.to_member();
            let result = parse_source(ParseQuery::new(id, members.member(id)));

            let errors: Vec<_> = result.diagnostics.iter().filter(|r| r.is_error()).collect();
            assert!(errors.is_empty(), "`{case}` failed to parse: {errors:#?}");
            assert!(result.node.is_some(), "`{case}` failed to lex");
        }
    }

    /// Likewise for the formatter.
    #[test]
    fn test_cases_format_without_errors() {
        for case in TestCase::formattable() {
            let (id, members) = case.to_parsed_member();
            let query = FormatQuery {
                options: FormatterOptions::default(),
                source: id,
                member: members.member(id),
            };
            let result = fmt_module(query);

            let errors: Vec<_> = result.diagnostics.iter().filter(|r| r.is_error()).collect();
            assert!(errors.is_empty(), "`{case}` failed to format: {errors:#?}");
        }
    }
}
