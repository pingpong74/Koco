#![allow(dead_code)]
use koco_span::{DiagnosticsCtx, Interner, SourceMap};
use koco_syntax::ast::visit::{self, Visitor};
use koco_syntax::ast::*;
use koco_syntax::{ParsedCrate, parse_crate};
use std::path::Path as FsPath;

pub struct Parsed {
    pub krate: ParsedCrate,
    pub interner: Interner,
    pub diags: DiagnosticsCtx,
    pub map: SourceMap,
}

pub fn parse(src: &str) -> Parsed {
    let mut map = SourceMap::new();
    let mut interner = Interner::new();
    let diags = DiagnosticsCtx::new();
    let file = map.add_file("test.kc", src);
    let krate = parse_crate(&mut map, &mut interner, &diags, file, FsPath::new("."));
    Parsed {
        krate,
        interner,
        diags,
        map,
    }
}

/// Parse, assert there were no diagnostics, and assert no arena node was orphaned.
pub fn parse_ok(src: &str) -> Parsed {
    let p = parse(src);
    assert!(p.diags.diagnostics().is_empty(), "unexpected diagnostics:\n{}", p.diags.format(&p.map));
    assert_fully_reachable(&p);
    p
}

#[derive(Default)]
struct Reach {
    items: std::collections::HashSet<Idx<Item>>,
    exprs: std::collections::HashSet<Idx<Expression>>,
    stmts: std::collections::HashSet<Idx<Statement>>,
    types: std::collections::HashSet<Idx<ParserType>>,
    pats: std::collections::HashSet<Idx<Pattern>>,
    paths: std::collections::HashSet<Idx<Path>>,
    variants: std::collections::HashSet<Idx<Variant>>,
    generic_params: std::collections::HashSet<Idx<GenericParam>>,
}

impl Visitor for Reach {
    fn visit_item(&mut self, a: &Arenas, id: Idx<Item>) {
        self.items.insert(id);
        visit::walk_item(self, a, id);
    }
    fn visit_generic_param(&mut self, a: &Arenas, id: Idx<GenericParam>) {
        self.generic_params.insert(id);
        visit::walk_generic_param(self, a, id);
    }
    fn visit_variant(&mut self, a: &Arenas, id: Idx<Variant>) {
        self.variants.insert(id);
        visit::walk_variant(self, a, id);
    }
    fn visit_stmt(&mut self, a: &Arenas, id: Idx<Statement>) {
        self.stmts.insert(id);
        visit::walk_stmt(self, a, id);
    }
    fn visit_expr(&mut self, a: &Arenas, id: Idx<Expression>) {
        self.exprs.insert(id);
        visit::walk_expr(self, a, id);
    }
    fn visit_pat(&mut self, a: &Arenas, id: Idx<Pattern>) {
        self.pats.insert(id);
        visit::walk_pat(self, a, id);
    }
    fn visit_type_ref(&mut self, a: &Arenas, id: Idx<ParserType>) {
        self.types.insert(id);
        visit::walk_type_ref(self, a, id);
    }
    fn visit_path(&mut self, a: &Arenas, id: Idx<Path>) {
        self.paths.insert(id);
        visit::walk_path(self, a, id);
    }
}

/// Panics if any arena holds an unreachable node, or if the walk reaches a node twice.
pub fn assert_fully_reachable(p: &Parsed) { assert_tree_fully_reachable(&p.krate.arenas, p.krate.root); }

/// [`assert_fully_reachable`] for tests that build a `Crate` without going through
/// [`parse`].
pub fn assert_tree_fully_reachable(a: &Arenas, root: Idx<Item>) {
    let mut r = Reach::default();
    r.visit_item(a, root);
    macro_rules! check {
        ($arena:expr, $seen:expr, $what:literal) => {
            assert_eq!($arena.len(), $seen.len(), "{} node(s) allocated but unreachable from the root ({what})", $arena.len() as i64 - $seen.len() as i64, what = $what,);
        };
    }
    check!(a.items, r.items, "items");
    check!(a.exprs, r.exprs, "exprs");
    check!(a.stmts, r.stmts, "stmts");
    check!(a.type_refs, r.types, "type_refs");
    check!(a.pats, r.pats, "pats");
    check!(a.paths, r.paths, "paths");
    check!(a.variants, r.variants, "variants");
    check!(a.generic_params, r.generic_params, "generic_params");
}

impl Parsed {
    pub fn root_items(&self) -> Vec<Idx<Item>> { self.krate.arenas.mod_items(self.krate.root).to_vec() }

    pub fn item(&self, id: Idx<Item>) -> &Item { &self.krate.arenas.items[id] }

    pub fn variant(&self, id: Idx<Variant>) -> &Variant { &self.krate.arenas.variants[id] }

    pub fn name(&self, s: koco_span::Symbol) -> &str { self.interner.resolve(s) }

    pub fn pat_name(&self, id: Idx<Pattern>) -> Option<&str> {
        match &self.krate.arenas.pats[id].kind {
            PatternKind::Binding { name, .. } => Some(self.name(*name)),
            _ => None,
        }
    }

    pub fn pat_mutable(&self, id: Idx<Pattern>) -> bool {
        matches!(
            self.krate.arenas.pats[id].kind,
            PatternKind::Binding {
                mutable: true,
                ..
            }
        )
    }

    /// Find a root-level fn by name.
    pub fn find_fn(&self, name: &str) -> &FnDef {
        for id in self.root_items() {
            if let ItemKind::Fn(f) = &self.item(id).kind {
                if self.name(f.name) == name {
                    return f;
                }
            }
        }
        panic!("no fn `{name}`");
    }

    pub fn block(&self, e: Idx<Expression>) -> &Block {
        match &self.krate.arenas.exprs[e].kind {
            ExpressionKind::Block(b) => b,
            other => panic!("not a block expression: {other:?}"),
        }
    }

    pub fn fn_body(&self, f: &FnDef) -> &Block { self.block(f.body.expect("fn has no body")) }

    /// S-expression rendering of an expression, for shape/precedence assertions.
    pub fn sexpr(&self, e: Idx<Expression>) -> String {
        let a = &self.krate.arenas;
        match &a.exprs[e].kind {
            ExpressionKind::Literal(Literal::Int(v)) => v.to_string(),
            ExpressionKind::Literal(Literal::Float(v)) => format!("{v}"),
            ExpressionKind::Literal(Literal::Bool(v)) => v.to_string(),
            ExpressionKind::Path(p) => self.path(*p),
            ExpressionKind::Unary { op, expr } => {
                format!(
                    "({} {})",
                    if *op == UnaryOp::Neg {
                        "neg"
                    } else {
                        "not"
                    },
                    self.sexpr(*expr)
                )
            }
            ExpressionKind::Binary {
                op,
                lhs,
                rhs,
            } => {
                format!("({:?} {} {})", op, self.sexpr(*lhs), self.sexpr(*rhs))
            }
            ExpressionKind::Assign {
                target,
                value,
            } => format!("(= {} {})", self.sexpr(*target), self.sexpr(*value)),
            ExpressionKind::AssignOp {
                op,
                target,
                value,
            } => {
                format!("({:?}= {} {})", op, self.sexpr(*target), self.sexpr(*value))
            }
            ExpressionKind::Ref {
                mutable,
                expr,
            } => {
                format!(
                    "({} {})",
                    if *mutable {
                        "&mut"
                    } else {
                        "&"
                    },
                    self.sexpr(*expr)
                )
            }
            ExpressionKind::Call {
                callee,
                args,
            } => {
                let args: Vec<_> = args.iter().map(|a| self.sexpr(*a)).collect();
                format!("(call {} [{}])", self.sexpr(*callee), args.join(" "))
            }
            ExpressionKind::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                let args: Vec<_> = args.iter().map(|a| self.sexpr(*a)).collect();
                format!("(mcall {} {} [{}])", self.sexpr(*receiver), self.name(*method), args.join(" "))
            }
            ExpressionKind::Field {
                base,
                field,
            } => {
                let f = match field {
                    FieldName::Named(s) => self.name(*s).to_string(),
                    FieldName::Index(i) => i.to_string(),
                };
                format!("(. {} {})", self.sexpr(*base), f)
            }
            ExpressionKind::Index {
                base,
                index,
            } => format!("(idx {} {})", self.sexpr(*base), self.sexpr(*index)),
            ExpressionKind::Cast { expr, .. } => format!("(as {})", self.sexpr(*expr)),
            ExpressionKind::Tuple(v) => {
                let v: Vec<_> = v.iter().map(|a| self.sexpr(*a)).collect();
                format!("(tuple {})", v.join(" "))
            }
            ExpressionKind::Array(v) => {
                let v: Vec<_> = v.iter().map(|a| self.sexpr(*a)).collect();
                format!("(array {})", v.join(" "))
            }
            other => format!("<{:?}>", std::mem::discriminant(other)),
        }
    }

    pub fn path(&self, p: Idx<Path>) -> String { self.krate.arenas.paths[p].segments.iter().map(|s| self.name(s.ident).to_string()).collect::<Vec<_>>().join("::") }

    /// The tail expression of a function body, rendered.
    pub fn tail_sexpr(&self, f: &FnDef) -> String {
        let block = self.fn_body(f);
        self.sexpr(block.tail.expect("body has no tail expression"))
    }

    /// Statement `i` of a block, expecting an `Expr` statement; returns its expression id.
    pub fn stmt_expr(&self, block: &Block, i: usize) -> Idx<Expression> {
        match self.krate.arenas.stmts[block.stmts[i]].kind {
            StatementKind::Expr { expr, .. } => expr,
            ref other => panic!("statement {i} is not an expression statement: {other:?}"),
        }
    }
}

pub fn error_messages(p: &Parsed) -> Vec<String> { p.diags.diagnostics().into_iter().map(|d| d.message).collect() }
