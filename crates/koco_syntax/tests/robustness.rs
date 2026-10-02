//! The parser must never panic, hang, or overflow the stack, whatever it is fed.
mod common;
use common::*;

const RICH: &str = r#"
use math::{sqrt, inner::*};
#[layout(set = 0, binding = 0)] static camera: Camera;
static mut tile: Shared<[f32; 256]>;
pub mod math { pub fn sqrt(x: f32) -> f32 { x } }
trait Add<R> { fn add(self, r: R) -> Self; const ZERO: f32; }
struct V<T, const N: u32> { pub data: [T; N], #[location(0)] p: (f32, f32) }
enum Shape { Empty, Circle(f32), Rect { w: f32, h: f32 } }
impl<T: Add<T>> V<T, 3> {
    fn scale(&mut self, k: f32, out: &mut f32, big: &V<T, 3>) -> f32 {
        let mut i = 0; let (a, b): (i32, i32) = (1, 2);
        while i < 10 { i += 1; if i == 3 { continue; } }
        let s = match self.data[0] { Shape::Circle(r) if r > 0.0 => r * 2.0, Shape::Rect { w, .. } => w, _ => -1.0 };
        let p = Point { x: 1.0, y }; foo::<f32>(&mut i, a as f32 << 2 >> 1);
        loop { break s + (b as f32) * 3.5e2; }
    }
}
#[compute(8, 8, 1)] fn main() { let v = vec3::<f32>(0.0, 1.0, 2.0); v.scale(2.0, &mut o, &v); }
"#;

#[test]
fn the_rich_sample_itself_parses_cleanly() { parse_ok(RICH); }

#[test]
fn every_prefix_of_a_program_is_handled() {
    let chars: Vec<usize> = RICH.char_indices().map(|(i, _)| i).collect();
    for end in chars {
        let _ = parse(&RICH[..end]); // must return, not panic or hang
    }
}

#[test]
fn deleting_any_single_token_is_handled() {
    let mut diags = koco_span::DiagnosticsCtx::new();
    let toks = koco_syntax::lex(RICH, koco_span::FileId::from(0), &mut diags);
    for skip in 0..toks.len() {
        let mut src = String::new();
        let mut last = 0;
        for (i, t) in toks.iter().enumerate() {
            if i == skip {
                src.push_str(&RICH[last..t.span.lo as usize]);
                last = t.span.hi as usize;
            }
        }
        src.push_str(&RICH[last..]);
        let _ = parse(&src);
    }
}

#[test]
fn duplicating_any_single_token_is_handled() {
    let diags = koco_span::DiagnosticsCtx::new();
    let toks = koco_syntax::lex(RICH, koco_span::FileId::from(0), &diags);
    for dup in 0..toks.len() {
        let t = toks[dup];
        let (lo, hi) = (t.span.lo as usize, t.span.hi as usize);
        let src = format!("{} {} {}", &RICH[..hi], &RICH[lo..hi], &RICH[hi..]);
        let _ = parse(&src);
    }
}

#[test]
fn absurd_nesting_is_an_error_not_a_stack_overflow() {
    // Run on a small-stack thread to make an overflow certain if the limit were missing.
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            for open in [
                "(", "[", "{", "-", "&", "!",
            ] {
                let src = format!("fn f() {{ {} }}", open.repeat(100_000));
                let p = parse(&src);
                assert!(p.diags.has_errors());
            }
            let p = parse(&format!("fn f(x: {}f32) {{}}", "(".repeat(100_000)));
            assert!(p.diags.has_errors());
            let p = parse(&format!("fn f() {{ match x {{ {}_ => 1 }} }}", "(".repeat(100_000)));
            assert!(p.diags.has_errors());
            let p = parse(&format!("struct S {{ a: {} }}", "[".repeat(100_000)));
            assert!(p.diags.has_errors());
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn reasonable_nesting_still_works() {
    let src = format!("fn f() -> i32 {{ {}1{} }}", "(".repeat(40), ")".repeat(40));
    parse_ok(&src);
    let src = format!("fn f() {{ {} {} }}", "if a {".repeat(30), "}".repeat(30));
    parse_ok(&src);
}

#[test]
fn absurd_item_and_use_nesting_is_an_error() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let p = parse(&"mod a {".repeat(100_000));
            assert!(p.diags.has_errors());
            let p = parse(&format!("use {}", "a::{".repeat(100_000)));
            assert!(p.diags.has_errors());
            let p = parse(&"fn f() { ".repeat(100_000));
            assert!(p.diags.has_errors());
            let p = parse(&format!("fn f() {{ {} }}", "loop {".repeat(100_000)));
            assert!(p.diags.has_errors());
            let p = parse(&format!("impl A {{ {} }}", "impl B {".repeat(100_000)));
            assert!(p.diags.has_errors());
            let p = parse(&format!("fn f(x: {}", "A<".repeat(100_000)));
            assert!(p.diags.has_errors());
        })
        .unwrap();
    handle.join().unwrap();
}
