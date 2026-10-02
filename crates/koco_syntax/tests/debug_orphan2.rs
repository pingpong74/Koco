mod common;
use common::*;

#[test]
fn narrow() {
    for src in [
        "fn a( { } fn ok() {}",
        "struct S { x } fn ok() {}",
        "fn ok() -> i32 { let = 3; 4 }",
        "fn f() { let x = 1 let y = 2; } fn ok() {}",
    ] {
        let p = parse(src);
        let a = &p.krate.arenas;
        print!("[{src}] exprs={} ", a.exprs.len());
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| assert_fully_reachable(&p))).map(|_| println!("OK")).unwrap_or_else(|_| println!("ORPHAN"));
    }
}
