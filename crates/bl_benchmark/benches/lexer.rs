use bl_benchmark::TestCase;
use bl_lexer::Lexer;
use divan::{AllocProfiler, Bencher, counter::BytesCount};

#[global_allocator]
static ALLOC: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench(args = TestCase::all())]
fn lexer(bencher: Bencher, case: &TestCase) {
    let (id, members) = case.to_member();
    let member = members.member(id);

    bencher
        .counter(BytesCount::of_str(&member.contents))
        .bench(|| Lexer::new(member.spanned(), id, member.dialect).tokenise());
}
