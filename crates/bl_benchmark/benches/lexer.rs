use bl_benchmark::TestCase;
use bl_lexer::Lexer;
use divan::Bencher;

// CodSpeed measures the allocations itself, in its memory mode.
#[cfg(not(codspeed))]
#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench(args = TestCase::all())]
fn lexer(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_member();
    let member = members.member(id);

    // CodSpeed has no counters, and warns about each one.
    #[cfg(not(codspeed))]
    let bencher = bencher.counter(divan::counter::BytesCount::of_str(&member.contents));

    bencher.bench(|| Lexer::new(member.spanned(), id, member.dialect).tokenise());
}
