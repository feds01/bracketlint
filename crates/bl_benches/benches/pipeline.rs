//! Benchmarks for the stages of the bracketlint pipeline: lexing, parsing and
//! formatting a template.
//!
//! Each stage runs on a Django and a Liquid template, which are repeated a
//! number of times to measure how the stage scales with the size of the input.

use std::path::PathBuf;

use bl_ast::{Dialect, SourceId};
use bl_fmt::{FormatQuery, FormatterOptions, fmt_module};
use bl_lexer::Lexer;
use bl_parse::{ParseQuery, parse_source};
use bl_workspace::WorkspaceMembers;
use divan::{Bencher, black_box};

fn main() {
    divan::main();
}

/// The number of times that a template is repeated in a benchmark input.
const SIZES: &[usize] = &[1, 16];

const DJANGO_PAGE: &str = include_str!("../fixtures/django_page.html");
const LIQUID_PAGE: &str = include_str!("../fixtures/liquid_page.html");

/// A template, in the dialect that it is written in.
#[derive(Clone, Copy)]
struct Template {
    contents: &'static str,
    dialect: Dialect,
}

const DJANGO: Template = Template { contents: DJANGO_PAGE, dialect: Dialect::Django };
const LIQUID: Template = Template { contents: LIQUID_PAGE, dialect: Dialect::Liquid };

/// Create a workspace with a single member, whose contents are the template
/// repeated `size` times.
fn workspace_with(template: Template, size: usize) -> (WorkspaceMembers, SourceId) {
    let mut members = WorkspaceMembers::new();
    let path = PathBuf::from(format!("bench.{}", template.dialect.extension().unwrap_or("html")));
    let id = members.reserve_member(path, template.contents.repeat(size), template.dialect);
    (members, id)
}

/// Create a workspace with a single member whose document has been parsed,
/// ready to be formatted.
fn parsed_workspace_with(template: Template, size: usize) -> (WorkspaceMembers, SourceId) {
    let (mut members, id) = workspace_with(template, size);
    let result = parse_source(ParseQuery::new(id, members.member(id)));
    assert!(
        result.diagnostics.iter().all(|report| !report.is_error()),
        "the benchmark template should parse without errors"
    );
    members.member_mut(id).document = result.node;
    (members, id)
}

fn bench_lex(bencher: Bencher, template: Template, size: usize) {
    let (members, id) = workspace_with(template, size);
    let member = members.member(id);

    bencher.bench_local(|| {
        black_box(Lexer::new(member.spanned(), id, member.dialect).tokenise());
    });
}

fn bench_parse(bencher: Bencher, template: Template, size: usize) {
    let (members, id) = workspace_with(template, size);
    let member = members.member(id);

    bencher.bench_local(|| {
        black_box(parse_source(ParseQuery::new(id, member)));
    });
}

fn bench_fmt(bencher: Bencher, template: Template, size: usize) {
    let (members, id) = parsed_workspace_with(template, size);
    let member = members.member(id);
    let options = FormatterOptions::default();

    let result = fmt_module(FormatQuery { options, source: id, member });
    assert!(
        result.diagnostics.iter().all(|report| !report.is_error()),
        "the benchmark template should format without errors"
    );

    bencher.bench_local(|| {
        black_box(fmt_module(FormatQuery { options, source: id, member }));
    });
}

mod lex {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn django(bencher: Bencher, size: usize) {
        bench_lex(bencher, DJANGO, size);
    }

    #[divan::bench(args = SIZES)]
    fn liquid(bencher: Bencher, size: usize) {
        bench_lex(bencher, LIQUID, size);
    }
}

mod parse {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn django(bencher: Bencher, size: usize) {
        bench_parse(bencher, DJANGO, size);
    }

    #[divan::bench(args = SIZES)]
    fn liquid(bencher: Bencher, size: usize) {
        bench_parse(bencher, LIQUID, size);
    }
}

mod fmt {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn django(bencher: Bencher, size: usize) {
        bench_fmt(bencher, DJANGO, size);
    }

    #[divan::bench(args = SIZES)]
    fn liquid(bencher: Bencher, size: usize) {
        bench_fmt(bencher, LIQUID, size);
    }
}
