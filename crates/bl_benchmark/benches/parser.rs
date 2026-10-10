use bl_ast::Arena;
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

/// Lexes and parses the template into an arena, as `bracketlint check` does,
/// which resets the arena before each file that it parses.
#[divan::bench(args = TestCase::all())]
fn parser(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_member();
    let member = members.member(id);
    let mut arena = Arena::new();

    // CodSpeed has no counters, and warns about each one.
    #[cfg(not(codspeed))]
    let bencher = bencher.counter(divan::counter::BytesCount::of_str(&member.contents));

    bencher.bench_local(|| {
        arena.reset();
        let result = parse_source(ParseQuery::new(id, member), &arena);

        // The tree borrows the arena, so only the diagnostics are returned, to
        // be dropped after the parse is timed.
        divan::black_box(result.node);
        result.diagnostics
    });
}
