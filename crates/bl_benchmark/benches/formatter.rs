use bl_benchmark::TestCase;
use bl_fmt::{FormatQuery, FormatterOptions, fmt_module};
use divan::Bencher;

// CodSpeed measures the allocations itself, in its memory mode.
#[cfg(not(codspeed))]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

/// Formats the parsed template, as `bracketlint fmt` does.
#[divan::bench(args = TestCase::formattable())]
fn formatter(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_parsed_member();
    let member = members.member(id);
    let options = FormatterOptions::default();

    // CodSpeed has no counters, and warns about each one.
    #[cfg(not(codspeed))]
    let bencher = bencher.counter(divan::counter::BytesCount::of_str(&member.contents));

    bencher.bench(|| fmt_module(FormatQuery { options, source: id, member }));
}
