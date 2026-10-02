mod common;
use common::*;
use koco_syntax::ast::*;

#[test]
fn empty_file() {
    let p = parse_ok("");
    assert!(p.root_items().is_empty());
}

#[test]
fn comments_are_skipped() {
    let p = parse_ok("// line\nfn a() {} /* block\n * comment */ fn b() {}");
    assert_eq!(p.root_items().len(), 2);
}

#[test]
fn binary_precedence_follows_rust() {
    let p = parse_ok("fn f() -> i32 { 1 + 2 * 3 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(Add 1 (Mul 2 3))");

    let p = parse_ok("fn f() -> bool { a || b && c == d + 1 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(Or a (And b (Eq c (Add d 1))))");

    let p = parse_ok("fn f() -> i32 { a & b ^ c | d << 2 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(BitOr (BitXor (BitAnd a b) c) (Shl d 2))");
}

#[test]
fn binary_operators_are_left_associative() {
    let p = parse_ok("fn f() -> i32 { 10 - 3 - 2 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(Sub (Sub 10 3) 2)");
}

#[test]
fn unary_and_cast_precedence() {
    // `as` binds tighter than `*` but looser than unary minus, like Rust.
    let p = parse_ok("fn f() -> f32 { -x as f32 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(as (neg x))");
    let p = parse_ok("fn f() -> f32 { a * b as f32 }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(Mul a (as b))");
}

#[test]
fn postfix_chains() {
    let p = parse_ok("fn f() { a.b[1].c(2, 3).d; }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    let e = p.stmt_expr(block, 0);
    assert_eq!(p.sexpr(e), "(. (mcall (idx (. a b) 1) c [2 3]) d)");
}

#[test]
fn tuple_field_access_including_float_split() {
    let p = parse_ok("fn f() { t.0; t.0.1; }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    assert_eq!(p.sexpr(p.stmt_expr(block, 0)), "(. t 0)");
    assert_eq!(p.sexpr(p.stmt_expr(block, 1)), "(. (. t 0) 1)");
}

#[test]
fn assignment_is_right_associative_and_lowest() {
    let p = parse_ok("fn f() { a = b = c + 1; x += 2; }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    assert_eq!(p.sexpr(p.stmt_expr(block, 0)), "(= a (= b (Add c 1)))");
    assert_eq!(p.sexpr(p.stmt_expr(block, 1)), "(Add= x 2)");
}

#[test]
fn long_assignment_chain_is_bounded_not_a_stack_overflow() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let src = format!("fn f() {{ {} 1; }}", "x = ".repeat(100_000));
            let p = parse(&src);
            assert!(p.diags.has_errors());
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn ref_expressions_in_call_arguments() {
    let p = parse_ok("fn f() { g(&a, &mut b.c, 3); }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    let e = p.stmt_expr(block, 0);
    assert_eq!(p.sexpr(e), "(call g [(& a) (&mut (. b c)) 3])");
}

#[test]
fn parameter_modes_and_self_receivers() {
    let p = parse_ok(
        "struct V { x: f32 }
         impl V {
             fn len(&self) -> f32 { self.x }
             fn scale(&mut self, k: f32) { self.x *= k; }
             fn take(self) -> f32 { self.x }
             fn bump(mut self, out: &mut f32, big: &V, mut n: i32) {}
         }",
    );
    let impl_item = p.root_items()[1];
    let ItemKind::Impl(imp) = &p.item(impl_item).kind else { panic!() };
    assert_eq!(imp.items.len(), 4);
    let fns: Vec<&FnDef> = imp
        .items
        .iter()
        .map(|id| match &p.item(*id).kind {
            ItemKind::Fn(f) => f,
            _ => panic!(),
        })
        .collect();
    assert_eq!(fns[0].self_param.unwrap().mode, ParamMode::RefShared);
    assert_eq!(fns[1].self_param.unwrap().mode, ParamMode::RefMut);
    assert_eq!(fns[1].params.len(), 1);
    assert_eq!(fns[2].self_param.unwrap().mode, ParamMode::Value);
    let s = fns[3].self_param.unwrap();
    assert_eq!(p.pat_name(s.pat), Some("self"));
    assert!(p.pat_mutable(s.pat) && s.mode == ParamMode::Value);
    assert_eq!(fns[3].params[0].mode, ParamMode::RefMut);
    assert_eq!(p.pat_name(fns[3].params[0].pat), Some("out"));
    assert_eq!(fns[3].params[1].mode, ParamMode::RefShared);
    assert_eq!(fns[3].params[2].mode, ParamMode::Value);
    assert!(p.pat_mutable(fns[3].params[2].pat));
    assert_eq!(p.pat_name(fns[3].params[2].pat), Some("n"));
}

#[test]
fn generics_traits_and_impls() {
    let p = parse_ok(
        "trait Add<Rhs> { fn add(self, rhs: Rhs) -> Self; const ZERO: f32; }
         trait Shape: Add<f32> + Copy { fn area(&self) -> f32; fn twice(&self) -> f32 { 2.0 * self.area() } }
         struct Wrapper<T, const N: u32> { data: [T; N] }
         impl<T: Add<T>, const N: u32> Shape for Wrapper<T, N> { fn area(&self) -> f32 { 1.0 } }
         fn sum<T: Add<T, Output = T> + Copy>(a: T, b: T) -> T { a.add(b) }
         fn main() { let x = sum::<f32>(1.0, 2.0); }",
    );
    let items = p.root_items();
    assert_eq!(items.len(), 6);

    let ItemKind::Trait(shape) = &p.item(items[1]).kind else { panic!() };
    assert_eq!(shape.supertraits.len(), 2);
    assert_eq!(shape.items.len(), 2);
    // required method has no body, provided method has one
    let ItemKind::Fn(area) = &p.item(shape.items[0]).kind else { panic!() };
    assert!(area.body.is_none());
    let ItemKind::Fn(twice) = &p.item(shape.items[1]).kind else { panic!() };
    assert!(twice.body.is_some());

    let ItemKind::Impl(imp) = &p.item(items[3]).kind else { panic!() };
    assert!(imp.trait_ref.is_some());
    assert_eq!(imp.generics.params.len(), 2);
    let arenas = &p.krate.arenas;
    assert!(matches!(arenas.generic_params[imp.generics.params[1]].kind, GenericParamKind::Const { .. }));

    let ItemKind::Fn(sum) = &p.item(items[4]).kind else { panic!() };
    let GenericParamKind::Type { bounds } = &arenas.generic_params[sum.generics.params[0]].kind else { panic!() };
    assert_eq!(bounds.len(), 2);
    // the first bound carries an associated-type binding `Output = T`
    assert!(matches!(arenas.paths[bounds[0]].segments[0].args[1], GenericArg::AssocBinding { .. }));
}

#[test]
fn nested_generic_closers_are_split() {
    // `>>` must be treated as two closing angle brackets in type position.
    let p = parse_ok("fn f(x: Buffer<Vec<f32>>) -> Option<Option<i32>> { y }");
    let f = p.find_fn("f");
    let arenas = &p.krate.arenas;
    let ParserTypeKind::Path(path) = &arenas.type_refs[f.params[0].ty].kind else { panic!() };
    assert_eq!(p.name(arenas.paths[*path].segments[0].ident), "Buffer");
    assert_eq!(arenas.paths[*path].segments[0].args.len(), 1);
}

#[test]
fn comparison_is_not_mistaken_for_generics() {
    let p = parse_ok("fn f() -> bool { a < b && c > d }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(And (Lt a b) (Gt c d))");
}

#[test]
fn turbofish_in_expressions_and_method_calls() {
    let p = parse_ok("fn f() { let a = foo::<f32, 3>(x); let b = x.cast::<u32>(); let c = Vec::<f32>::new(); }");
    let f = p.find_fn("f");
    assert_eq!(p.fn_body(f).stmts.len(), 3);
}

#[test]
fn types_arrays_tuples_and_const_args() {
    let p = parse_ok("struct S { a: [f32; 4], b: [[i32; 2]; N], c: (i32, f32), d: (), e: Buffer<f32>, f: [u32; { N + 1 }] }");
    let ItemKind::Struct(s) = &p.item(p.root_items()[0]).kind else { panic!() };
    let Fields::Named(fields) = &s.fields else { panic!() };
    assert_eq!(fields.len(), 6);
    let arenas = &p.krate.arenas;
    let ty = |i: usize| &arenas.type_refs[fields[i].ty].kind;
    assert!(matches!(ty(0), ParserTypeKind::Array(_, ConstArg::Literal(Literal::Int(4), _))));
    assert!(matches!(ty(1), ParserTypeKind::Array(_, ConstArg::Path(_))));
    assert!(matches!(ty(2), ParserTypeKind::Tuple(v) if v.len() == 2));
    assert!(matches!(ty(3), ParserTypeKind::Tuple(v) if v.is_empty()));
    assert!(matches!(ty(5), ParserTypeKind::Array(_, ConstArg::Block(_))));
}

#[test]
fn structs_and_enums_of_every_shape() {
    let p = parse_ok(
        "struct Unit;
         struct Pair(f32, f32);
         struct Point<T> { pub x: T, y: T }
         enum Shape { Empty, Circle(f32), Rect { w: f32, h: f32 }, Poly(Vec<f32>, u32) }",
    );
    let items = p.root_items();
    let ItemKind::Struct(u) = &p.item(items[0]).kind else { panic!() };
    assert!(matches!(u.fields, Fields::Unit));
    let ItemKind::Struct(pair) = &p.item(items[1]).kind else { panic!() };
    assert!(matches!(&pair.fields, Fields::Tuple(f) if f.len() == 2));
    let ItemKind::Struct(pt) = &p.item(items[2]).kind else { panic!() };
    let Fields::Named(fields) = &pt.fields else { panic!() };
    assert_eq!(fields[0].vis, Visibility::Public);
    assert_eq!(fields[1].vis, Visibility::Private);
    let ItemKind::Enum(e) = &p.item(items[3]).kind else { panic!() };
    assert_eq!(e.variants.len(), 4);
    assert!(matches!(p.variant(e.variants[0]).fields, Fields::Unit));
    assert!(matches!(&p.variant(e.variants[1]).fields, Fields::Tuple(f) if f.len() == 1));
    assert!(matches!(&p.variant(e.variants[2]).fields, Fields::Named(f) if f.len() == 2));
    assert!(matches!(&p.variant(e.variants[3]).fields, Fields::Tuple(f) if f.len() == 2));
}

#[test]
fn attributes_are_parsed_generically() {
    let p = parse_ok(
        "#[vertex] fn vs() {}
         #[compute(8, 8, 1)] fn cs() {}
         #[layout(set = 0, binding = 1)] static camera: Camera;
         #[push_constant] static pc: Push;
         struct V { #[location(0)] pos: f32, #[builtin(position)] p: f32 }",
    );
    let items = p.root_items();
    let compute = &p.item(items[1]).attrs[0];
    assert_eq!(p.name(compute.name), "compute");
    assert_eq!(compute.args.len(), 3);
    assert_eq!(compute.args[0].value, AttrValue::Int(8));
    let layout = &p.item(items[2]).attrs[0];
    assert_eq!(p.name(layout.args[0].key.unwrap()), "set");
    assert_eq!(layout.args[1].value, AttrValue::Int(1));
    assert!(matches!(p.item(items[2]).kind, ItemKind::Static(_)));
    let ItemKind::Struct(s) = &p.item(items[4]).kind else { panic!() };
    let Fields::Named(fields) = &s.fields else { panic!() };
    assert_eq!(p.name(fields[1].attrs[0].name), "builtin");
}

#[test]
fn statics_consts_and_aliases() {
    let p = parse_ok(
        "const PI: f32 = 3.14159;
         static mut TILE: Shared<[f32; 256]>;
         static COUNT: u32 = 4;
         type Vec3 = [f32; 3];
         type Pair<T> = (T, T);",
    );
    let items = p.root_items();
    assert_eq!(items.len(), 5);
    let ItemKind::Static(t) = &p.item(items[1]).kind else { panic!() };
    assert!(t.mutable && t.init.is_none());
    let ItemKind::Static(c) = &p.item(items[2]).kind else { panic!() };
    assert!(!c.mutable && c.init.is_some());
}

#[test]
fn modules_use_trees_and_visibility() {
    let p = parse_ok(
        "pub mod math {
             pub(crate) fn sqrt(x: f32) -> f32 { x }
             pub(super) fn helper() {}
             mod inner { pub fn deep() {} }
         }
         use math::sqrt;
         use math::{sqrt as root, helper, inner::*};
         use crate::math::inner::deep;",
    );
    let items = p.root_items();
    assert_eq!(items.len(), 4);
    let ItemKind::Mod(_) = &p.item(items[0]).kind else { panic!() };
    assert_eq!(p.item(items[0]).vis, Visibility::Public);
    let children = p.krate.arenas.mod_items(items[0]);
    assert_eq!(p.item(children[0]).vis, Visibility::Crate);
    assert_eq!(p.item(children[1]).vis, Visibility::Super);
    assert_eq!(p.item(children[2]).vis, Visibility::Private);

    let ItemKind::Use(t) = &p.item(items[2]).kind else { panic!() };
    assert_eq!(t.prefix.len(), 1);
    let UseTreeKind::Nested(inner) = &t.kind else { panic!() };
    assert_eq!(inner.len(), 3);
    assert!(matches!(
        inner[0].kind,
        UseTreeKind::Simple {
            rename: Some(_)
        }
    ));
    assert!(matches!(inner[2].kind, UseTreeKind::Glob));
    assert_eq!(inner[2].prefix.len(), 1);
}

#[test]
fn outline_module_without_a_file_is_kept_as_outline_when_no_io_needed() {
    // Parsing alone never touches the filesystem for inline modules.
    let p = parse_ok("mod a { mod b { fn f() {} } }");
    assert_eq!(p.root_items().len(), 1);
}

#[test]
fn control_flow_and_statement_forms() {
    let p = parse_ok(
        "fn f(n: i32) -> i32 {
             let mut i = 0;
             let (a, b): (i32, i32) = (1, 2);
             while i < n { i += 1; if i == 3 { continue; } }
             loop { if i > 10 { break; } i = i + 1; }
             if a > b { return a; } else if a < b { return b; } else { return 0; }
         }
         fn g(x: i32) -> i32 { if x > 0 { 1 } else { 2 } }
         fn h() { let v = loop { break 5; }; }",
    );
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    // let, let, while, loop, if/else-if chain (a trailing block-like with no `;` is the tail)
    assert_eq!(block.stmts.len(), 4);
    assert!(block.tail.is_some());
    let g = p.find_fn("g");
    assert!(p.fn_body(g).tail.is_some());
}

#[test]
fn long_else_if_chain_is_bounded_not_a_stack_overflow() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let src = format!("fn f() {{ if a {{}} {} }}", "else if a {} ".repeat(100_000));
            let p = parse(&src);
            assert!(p.diags.has_errors());
        })
        .unwrap();
    handle.join().unwrap();
}

#[test]
fn block_like_statement_is_not_continued_by_operators() {
    // `if ... {}` followed by `-1` is a statement then a separate expression.
    let p = parse_ok("fn f() -> i32 { if a { b; } -1 }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    assert_eq!(block.stmts.len(), 1);
    assert_eq!(p.sexpr(block.tail.unwrap()), "(neg 1)");
}

#[test]
fn struct_literals_and_the_no_struct_context() {
    let p = parse_ok(
        "fn f() {
             let p = Point { x: 1.0, y };
             let q = Empty {};
             if a == b { c; }
             while p.x < limit { d; }
             match s { Shape::Empty => 1, _ => 2 };
         }",
    );
    let f = p.find_fn("f");
    assert_eq!(p.fn_body(f).stmts.len(), 5);
}

#[test]
fn match_with_every_pattern_kind() {
    let p = parse_ok(
        "fn f(s: Shape, n: i32) -> f32 {
             match s {
                 Shape::Empty => 0.0,
                 Shape::Circle(r) => r,
                 Shape::Rect { w, h: height } if w > 0.0 => w * height,
                 Shape::Rect { .. } => 1.0,
                 other @ Shape::Poly(_, _) => 2.0,
                 _ => { 3.0 }
             }
         }
         fn g(n: i32) -> i32 {
             match n { 0 | 1 => 10, -1 => 20, x => x, }
         }
         fn h(t: (i32, bool)) -> i32 { match t { (1, true) => 1, (mut a, _) => a } }",
    );
    let f = p.find_fn("f");
    let arenas = &p.krate.arenas;
    let ExpressionKind::Match { arms, .. } = &arenas.exprs[p.fn_body(f).tail.unwrap()].kind else { panic!() };
    assert_eq!(arms.len(), 6);
    assert!(arms[2].guard.is_some());
    let pat = |i: usize| &arenas.pats[arms[i].pat].kind;
    assert!(matches!(pat(0), PatternKind::Path(_)));
    assert!(matches!(pat(1), PatternKind::TupleStruct { fields, .. } if fields.len() == 1));
    assert!(matches!(pat(2), PatternKind::Struct { rest: false, fields, .. } if fields.len() == 2));
    assert!(matches!(
        pat(3),
        PatternKind::Struct {
            rest: true,
            ..
        }
    ));
    assert!(matches!(
        pat(4),
        PatternKind::Binding {
            sub: Some(_),
            ..
        }
    ));
    assert!(matches!(pat(5), PatternKind::Wildcard));

    let g = p.find_fn("g");
    let ExpressionKind::Match { arms, .. } = &arenas.exprs[p.fn_body(g).tail.unwrap()].kind else { panic!() };
    assert!(matches!(&arenas.pats[arms[0].pat].kind, PatternKind::Or(v) if v.len() == 2));
    assert!(matches!(
        &arenas.pats[arms[1].pat].kind,
        PatternKind::Literal {
            negative: true,
            ..
        }
    ));
}

#[test]
fn self_in_a_pattern_is_never_treated_as_a_fresh_binding() {
    // `Self` (capital) is always a path pattern; lowercase `self` as a pattern (which
    // only makes grammatical sense nested in something like `Self(self) => ...`) must
    // resolve to the *existing* `self` binding, not shadow it with a new one.
    let p = parse_ok("impl S { fn f(self) -> i32 { match self { self => 1 } } }");
    let arenas = &p.krate.arenas;
    let ItemKind::Impl(imp) = &p.item(p.root_items()[0]).kind else { panic!() };
    let ItemKind::Fn(f) = &p.item(imp.items[0]).kind else { panic!() };
    let ExpressionKind::Match { arms, .. } = &arenas.exprs[p.fn_body(f).tail.unwrap()].kind else { panic!() };
    assert!(matches!(&arenas.pats[arms[0].pat].kind, PatternKind::Path(_)));
}

#[test]
fn literals() {
    let p = parse_ok("fn f() { 1_000; 0xFF; 3.5; 1e3; 2.5e-2; true; false; }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    let arenas = &p.krate.arenas;
    let lit = |i: usize| match &arenas.exprs[p.stmt_expr(block, i)].kind {
        ExpressionKind::Literal(l) => *l,
        _ => panic!(),
    };
    assert_eq!(lit(0), Literal::Int(1000));
    assert_eq!(lit(1), Literal::Int(255));
    assert_eq!(lit(2), Literal::Float(3.5));
    assert_eq!(lit(3), Literal::Float(1000.0));
    assert_eq!(lit(4), Literal::Float(0.025));
    assert_eq!(lit(5), Literal::Bool(true));
}

#[test]
fn items_inside_function_bodies() {
    let p = parse_ok("fn f() { const K: i32 = 3; fn helper() -> i32 { 1 } struct L { a: i32 } helper() }");
    let f = p.find_fn("f");
    let block = p.fn_body(f);
    assert_eq!(block.stmts.len(), 3);
    assert!(block.tail.is_some());
}

#[test]
fn line_comments_in_every_position() {
    // no trailing newline, CRLF line endings, `///` doc-style, and comment-only file
    parse_ok("fn a() {} // trailing, no newline");
    parse_ok("// only a comment");
    parse_ok("fn a() {\r\n    let x = 1; // note\r\n    x\r\n}\r\n");
    parse_ok("/// doc comment\nfn a() {}\n//// four slashes\nfn b() {}");
    parse_ok("fn f() -> i32 { 1 // one\n + 2 }");
    let p = parse_ok("fn f() { a // c\n = 1; }");
    assert_eq!(p.fn_body(p.find_fn("f")).stmts.len(), 1);
}

#[test]
fn slash_operators_are_not_mistaken_for_comments() {
    let p = parse_ok("fn f() -> i32 { a / b }");
    assert_eq!(p.tail_sexpr(p.find_fn("f")), "(Div a b)");
    let p = parse_ok("fn f() { x /= 2; y = a / /* inline */ b; }");
    assert_eq!(p.fn_body(p.find_fn("f")).stmts.len(), 2);
}
