use bl_benchmark::TestCase;
use bl_parse::{ParseQuery, parse_source};
use divan::Bencher;

// CodSpeed measures the allocations itself, in its memory mode.
#[cfg(not(codspeed))]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

/// Lexes and parses the template, as `bracketlint check` does.
#[divan::bench(args = TestCase::all())]
fn parser(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_member();
    let member = members.member(id);

    // CodSpeed has no counters, and warns about each one.
    #[cfg(not(codspeed))]
    let bencher = bencher.counter(divan::counter::BytesCount::of_str(&member.contents));

    bencher.bench(|| parse_source(ParseQuery::new(id, member)));
}
