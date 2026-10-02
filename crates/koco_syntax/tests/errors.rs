mod common;
use common::*;
use koco_syntax::ast::*;

#[test]
fn reference_types_are_rejected_outside_parameters() {
    let p = parse("fn f() { let x: &mut f32 = y; }");
    let msgs = error_messages(&p);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(msgs[0].contains("only allowed as function parameter types"));

    let p = parse("struct S { r: &f32 }");
    assert!(error_messages(&p)[0].contains("only allowed as function parameter types"));

    let p = parse("fn f() -> &f32 { x }");
    assert!(error_messages(&p)[0].contains("only allowed as function parameter types"));
}

#[test]
fn double_reference_is_rejected() {
    let p = parse("fn f(x: &&f32) {}");
    assert!(error_messages(&p)[0].contains("references to references"));
}

#[test]
fn if_let_and_while_let_have_helpful_errors() {
    let p = parse("fn f() { if let Some(x) = y { } }");
    assert!(error_messages(&p)[0].contains("`if let` is not supported yet"));
    let p = parse("fn f() { while let Some(x) = y { } }");
    assert!(error_messages(&p)[0].contains("`while let` is not supported yet"));
}

#[test]
fn missing_semicolon_is_reported_with_position() {
    let p = parse("fn f() { let x = 1 let y = 2; }");
    let diags = p.diags.diagnostics();
    assert!(!diags.is_empty());
    assert!(diags[0].message.contains("expected"), "{}", diags[0].message);
    let span = diags[0].primary.expect("error has a span");
    assert_eq!(p.map.snippet(span), "let");
}

#[test]
fn parser_recovers_and_reports_several_errors() {
    let src = "fn a( { }\n\
               fn ok1() {}\n\
               struct S { x }\n\
               fn ok2() -> i32 { let = 3; 4 }\n\
               fn ok3() {}\n";
    let p = parse(src);
    let n = p.diags.error_count();
    assert!(n >= 3, "expected at least 3 errors, got {n}:\n{}", p.diags.format(&p.map));
    // Recovery must keep the well-formed items.
    let names: Vec<String> = p.root_items().iter().filter_map(|id| p.item(*id).name()).map(|s| p.name(s).to_string()).collect();
    assert!(names.contains(&"ok1".to_string()), "{names:?}");
    assert!(names.contains(&"ok3".to_string()), "{names:?}");
}

#[test]
fn stray_tokens_do_not_hang_or_panic() {
    for src in [
        "}",
        "}}}",
        "fn",
        "fn f",
        "fn f(",
        "fn f() {",
        "struct",
        "impl",
        "mod",
        "use",
        "#",
        "#[",
        "match",
        "let",
        "1 + ",
        "fn f() { ( }",
        "fn f() { [1, }",
        "@@@ fn ok() {}",
    ] {
        let p = parse(src);
        assert!(p.diags.has_errors(), "`{src}` should produce an error");
    }
}

#[test]
fn unrecognised_characters_are_reported_once_per_run() {
    let p = parse("fn f() { let x = 1 $$$ 2; }");
    let msgs = error_messages(&p);
    assert!(msgs.iter().any(|m| m.contains("unrecognized character(s) `$$$`")), "{msgs:?}");
}

#[test]
fn integer_overflow_is_an_error() {
    let p = parse("fn f() { 99999999999999999999999; }");
    assert!(error_messages(&p)[0].contains("invalid integer literal"));
}

#[test]
fn tuple_field_index_overflow_is_an_error_not_a_panic() {
    let p = parse("fn f() { t.99999999999; }");
    assert!(error_messages(&p)[0].contains("tuple field index is too large"));
}

#[test]
fn non_fn_non_const_items_inside_impl_are_accepted_syntactically() {
    // Rejecting these is a semantic rule for `koco_hir`, not a syntax-tree shape rule.
    let p = parse_ok("impl S { struct Inner; fn ok() {} }");
    let ItemKind::Impl(imp) = &p.item(p.root_items()[0]).kind else { panic!() };
    assert_eq!(imp.items.len(), 2);
    assert!(matches!(p.item(imp.items[0]).kind, ItemKind::Struct(_)));
}

#[test]
fn non_path_before_for_in_impl_is_reported_without_orphaning_anything() {
    // The part before `for` must be a trait *path*; an array type cannot start one.
    let p = parse("impl [f32; 3] for S {}");
    assert!(p.diags.has_errors());
    assert_fully_reachable(&p);
}

#[test]
fn diagnostics_render_with_source_snippets() {
    let p = parse("fn f() {\n    let x = ;\n}\n");
    let text = p.diags.format(&p.map);
    assert!(text.contains("error: expected an expression, found `;`"), "{text}");
    assert!(text.contains("test.kc:2:13"), "{text}");
    assert!(text.contains("let x = ;"), "{text}");
}

#[test]
fn unterminated_block_comment_is_an_error_not_a_hang() {
    let p = parse("fn f() {} /* never closed");
    assert!(p.diags.has_errors());
}

#[test]
fn recovery_may_leave_unreachable_nodes_behind_but_never_panics() {
    // A statement that fails partway through (`let x = 1 let y = 2;`) leaves the nodes it
    // already allocated behind, since the failed statement never joins the tree. What
    // matters is that recovery never panics or corrupts the reachable tree.
    for src in [
        "fn a( { } fn ok() {}",
        "struct S { x } fn ok() {}",
        "fn ok() -> i32 { let = 3; 4 }",
        "fn f() { let x = 1 let y = 2; } fn ok() {}",
    ] {
        let p = parse(src);
        assert!(p.diags.has_errors(), "`{src}` should have produced an error");
    }
}
