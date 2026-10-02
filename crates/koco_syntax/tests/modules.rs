mod common;
use common::assert_tree_fully_reachable;
use koco_span::{DiagnosticsCtx, Interner, SourceMap};
use koco_syntax::ast::*;
use koco_syntax::parse_crate;
use std::fs;
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("koco_syntax_test_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn load(dir: &PathBuf, lib: &str) -> (koco_syntax::ParsedCrate, Interner, DiagnosticsCtx, SourceMap) {
    let mut map = SourceMap::new();
    let mut interner = Interner::new();
    let diags = DiagnosticsCtx::new();
    let lib_path = dir.join("lib.rs");
    fs::write(&lib_path, lib).unwrap();
    let file = map.add_file(lib_path.display().to_string(), lib);
    let krate = parse_crate(&mut map, &mut interner, &diags, file, dir);
    (krate, interner, diags, map)
}

fn children(krate: &koco_syntax::ParsedCrate, id: Idx<Item>) -> Vec<Idx<Item>> { krate.arenas.mod_items(id).to_vec() }

#[test]
fn outline_modules_are_loaded_recursively_from_files() {
    let dir = temp_dir("nested");
    fs::write(dir.join("a.rs"), "pub mod b; pub fn from_a() {}").unwrap();
    fs::create_dir_all(dir.join("a")).unwrap();
    fs::write(dir.join("a").join("b.rs"), "pub fn from_b() {}").unwrap();
    fs::write(dir.join("c.rs"), "pub fn from_c() {}").unwrap();

    let (krate, interner, diags, map) = load(&dir, "mod a; mod c; fn top() {}");
    assert!(!diags.has_errors(), "{}", diags.format(&map));
    assert_tree_fully_reachable(&krate.arenas, krate.root);

    let root = children(&krate, krate.root);
    assert_eq!(root.len(), 3);

    // a.rs is spliced in as `mod a`'s body; its own `mod b;` was loaded from a/b.rs.
    let a_items = children(&krate, root[0]);
    assert_eq!(a_items.len(), 2);
    let b_items = children(&krate, a_items[0]);
    assert_eq!(b_items.len(), 1);
    let ItemKind::Fn(f) = &krate.arenas.items[b_items[0]].kind else {
        panic!()
    };
    assert_eq!(interner.resolve(f.name), "from_b");

    // Every file is registered in the source map, and spans point into the right one.
    let span = krate.arenas.items[b_items[0]].span;
    assert!(map.name(span.file).ends_with("b.rs"));
    assert_eq!(map.snippet(span), "pub fn from_b() {}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn modules_nested_inline_look_in_subdirectories() {
    let dir = temp_dir("inline_nested");
    fs::create_dir_all(dir.join("outer")).unwrap();
    fs::write(dir.join("outer").join("leaf.rs"), "fn leaf() {}").unwrap();

    let (krate, _interner, diags, map) = load(&dir, "mod outer { mod leaf; }");
    assert!(!diags.has_errors(), "{}", diags.format(&map));
    assert_tree_fully_reachable(&krate.arenas, krate.root);
    let outer = children(&krate, krate.root)[0];
    let leaf = children(&krate, outer)[0];
    assert_eq!(children(&krate, leaf).len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn missing_module_file_is_a_diagnostic_pointing_at_the_mod_item() {
    let dir = temp_dir("missing");
    let (krate, _interner, diags, map) = load(&dir, "fn ok() {}\nmod nope;\n");
    assert_eq!(diags.error_count(), 1, "{}", diags.format(&map));
    let d = &diags.diagnostics()[0];
    assert!(d.message.contains("cannot load module `nope`"));
    assert_eq!(map.snippet(d.primary.unwrap()), "mod nope;");
    // The rest of the program is still available.
    assert_eq!(children(&krate, krate.root).len(), 2);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn syntax_errors_inside_loaded_files_point_at_that_file() {
    let dir = temp_dir("err_in_child");
    fs::write(dir.join("bad.rs"), "fn broken( {").unwrap();
    let (_krate, _interner, diags, map) = load(&dir, "mod bad;");
    assert!(diags.has_errors());
    let d = &diags.diagnostics()[0];
    assert!(map.name(d.primary.unwrap().file).ends_with("bad.rs"));
    let rendered = diags.format(&map);
    assert!(rendered.contains("bad.rs"), "{rendered}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ids_are_unique_across_files() {
    // Items from different files live in one arena, so ids never collide.
    let dir = temp_dir("ids");
    fs::write(dir.join("m.rs"), "fn one() {} fn two() {}").unwrap();
    let (krate, _i, diags, _m) = load(&dir, "mod m; fn three() {}");
    assert!(!diags.has_errors());
    let mut seen = std::collections::HashSet::new();
    fn walk(k: &koco_syntax::ParsedCrate, id: Idx<Item>, seen: &mut std::collections::HashSet<Idx<Item>>) {
        assert!(seen.insert(id));
        if matches!(k.arenas.items[id].kind, ItemKind::Mod(_)) {
            for c in k.arenas.mod_items(id) {
                walk(k, *c, seen);
            }
        }
    }
    walk(&krate, krate.root, &mut seen);
    assert_eq!(seen.len(), 5); // root, m, one, two, three
    let _ = fs::remove_dir_all(&dir);
}
