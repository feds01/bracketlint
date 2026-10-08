use bl_benchmark::TestCase;
use bl_parse::{ParseQuery, parse_source};
use divan::{AllocProfiler, Bencher, counter::BytesCount};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

/// Lexes and parses the template, as `bracketlint check` does.
///
/// ##Note: Each parse adds the span of every node to the global `SpanMap`, which
/// is never cleared, so it grows with each iteration and with each case run
/// before this one.
#[divan::bench(args = TestCase::all())]
fn parser(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_member();
    let member = members.member(id);

    bencher
        .counter(BytesCount::of_str(&member.contents))
        .bench(|| parse_source(ParseQuery::new(id, member)));
}
