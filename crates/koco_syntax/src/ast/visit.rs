//! A read-only walker over the tree. The tree shape is encoded once, in the `walk_*`
//! functions: override the `visit_*` methods you care about and call the matching
//! `walk_*` to continue into the children.

use super::*;

pub trait Visitor: Sized {
    fn visit_item(&mut self, a: &Arenas, id: Idx<Item>) { walk_item(self, a, id) }
    fn visit_generic_param(&mut self, a: &Arenas, id: Idx<GenericParam>) { walk_generic_param(self, a, id) }
    fn visit_variant(&mut self, a: &Arenas, id: Idx<Variant>) { walk_variant(self, a, id) }
    fn visit_stmt(&mut self, a: &Arenas, id: Idx<Statement>) { walk_stmt(self, a, id) }
    fn visit_expr(&mut self, a: &Arenas, id: Idx<Expression>) { walk_expr(self, a, id) }
    fn visit_pat(&mut self, a: &Arenas, id: Idx<Pattern>) { walk_pat(self, a, id) }
    fn visit_type_ref(&mut self, a: &Arenas, id: Idx<ParserType>) { walk_type_ref(self, a, id) }
    fn visit_path(&mut self, a: &Arenas, id: Idx<Path>) { walk_path(self, a, id) }
}

pub fn walk_generics<V: Visitor>(v: &mut V, a: &Arenas, generics: &Generics) {
    for &p in &generics.params {
        v.visit_generic_param(a, p);
    }
}

pub fn walk_generic_param<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<GenericParam>) {
    match &a.generic_params[id].kind {
        GenericParamKind::Type { bounds } => {
            for &b in bounds {
                v.visit_path(a, b);
            }
        }
        GenericParamKind::Const { ty } => v.visit_type_ref(a, *ty),
    }
}

pub fn walk_fields<V: Visitor>(v: &mut V, a: &Arenas, fields: &Fields) {
    for f in fields.defs() {
        v.visit_type_ref(a, f.ty);
    }
}

pub fn walk_variant<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Variant>) { walk_fields(v, a, &a.variants[id].fields); }

pub fn walk_item<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Item>) {
    match &a.items[id].kind {
        ItemKind::Fn(f) => {
            walk_generics(v, a, &f.generics);
            if let Some(s) = &f.self_param {
                v.visit_pat(a, s.pat);
            }
            for p in &f.params {
                v.visit_pat(a, p.pat);
                v.visit_type_ref(a, p.ty);
            }
            if let Some(t) = f.ret_ty {
                v.visit_type_ref(a, t);
            }
            if let Some(b) = f.body {
                v.visit_expr(a, b);
            }
        }
        ItemKind::Struct(s) => {
            walk_generics(v, a, &s.generics);
            walk_fields(v, a, &s.fields);
        }
        ItemKind::Enum(e) => {
            walk_generics(v, a, &e.generics);
            for &var in &e.variants {
                v.visit_variant(a, var);
            }
        }
        ItemKind::Trait(t) => {
            walk_generics(v, a, &t.generics);
            for &s in &t.supertraits {
                v.visit_path(a, s);
            }
            for &i in &t.items {
                v.visit_item(a, i);
            }
        }
        ItemKind::Impl(i) => {
            walk_generics(v, a, &i.generics);
            if let Some(t) = i.trait_ref {
                v.visit_path(a, t);
            }
            v.visit_type_ref(a, i.self_ty);
            for &item in &i.items {
                v.visit_item(a, item);
            }
        }
        ItemKind::Mod(m) => {
            if let ModKind::Inline(items) = &m.kind {
                for &i in items {
                    v.visit_item(a, i);
                }
            }
        }
        ItemKind::Use(_) => {}
        ItemKind::Const(c) => {
            v.visit_type_ref(a, c.ty);
            if let Some(e) = c.value {
                v.visit_expr(a, e);
            }
        }
        ItemKind::Static(s) => {
            v.visit_type_ref(a, s.ty);
            if let Some(e) = s.init {
                v.visit_expr(a, e);
            }
        }
        ItemKind::TypeAlias(t) => {
            walk_generics(v, a, &t.generics);
            v.visit_type_ref(a, t.ty);
        }
    }
}

pub fn walk_type_ref<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<ParserType>) {
    match &a.type_refs[id].kind {
        ParserTypeKind::Path(p) => v.visit_path(a, *p),
        ParserTypeKind::Array(elem, len) => {
            v.visit_type_ref(a, *elem);
            walk_const_arg(v, a, len);
        }
        ParserTypeKind::Tuple(elems) => {
            for &e in elems {
                v.visit_type_ref(a, e);
            }
        }
    }
}

pub fn walk_const_arg<V: Visitor>(v: &mut V, a: &Arenas, arg: &ConstArg) {
    match arg {
        ConstArg::Literal(..) => {}
        ConstArg::Path(p) => v.visit_path(a, *p),
        ConstArg::Block(e) => v.visit_expr(a, *e),
    }
}

pub fn walk_generic_args<V: Visitor>(v: &mut V, a: &Arenas, args: &[GenericArg]) {
    for arg in args {
        match arg {
            GenericArg::Type(t) => v.visit_type_ref(a, *t),
            GenericArg::Const(c) => walk_const_arg(v, a, c),
            GenericArg::AssocBinding { ty, .. } => v.visit_type_ref(a, *ty),
        }
    }
}

pub fn walk_path<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Path>) {
    for seg in &a.paths[id].segments {
        walk_generic_args(v, a, &seg.args);
    }
}

pub fn walk_pat<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Pattern>) {
    match &a.pats[id].kind {
        PatternKind::Wildcard | PatternKind::Literal { .. } => {}
        PatternKind::Binding { sub, .. } => {
            if let Some(s) = sub {
                v.visit_pat(a, *s);
            }
        }
        PatternKind::Path(p) => v.visit_path(a, *p),
        PatternKind::TupleStruct {
            path,
            fields,
        } => {
            v.visit_path(a, *path);
            for &f in fields {
                v.visit_pat(a, f);
            }
        }
        PatternKind::Struct {
            path,
            fields,
            ..
        } => {
            v.visit_path(a, *path);
            for f in fields {
                v.visit_pat(a, f.pat);
            }
        }
        PatternKind::Tuple(pats) | PatternKind::Or(pats) => {
            for &p in pats {
                v.visit_pat(a, p);
            }
        }
    }
}

pub fn walk_stmt<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Statement>) {
    match &a.stmts[id].kind {
        StatementKind::Let {
            pat,
            ty,
            init,
        } => {
            v.visit_pat(a, *pat);
            if let Some(t) = ty {
                v.visit_type_ref(a, *t);
            }
            if let Some(e) = init {
                v.visit_expr(a, *e);
            }
        }
        StatementKind::Expr { expr, .. } => v.visit_expr(a, *expr),
        StatementKind::Item(i) => v.visit_item(a, *i),
    }
}

pub fn walk_expr<V: Visitor>(v: &mut V, a: &Arenas, id: Idx<Expression>) {
    match &a.exprs[id].kind {
        ExpressionKind::Literal(_) | ExpressionKind::Continue => {}
        ExpressionKind::Path(p) => v.visit_path(a, *p),
        ExpressionKind::Unary { expr, .. } | ExpressionKind::Ref { expr, .. } => v.visit_expr(a, *expr),
        ExpressionKind::Binary {
            lhs, rhs, ..
        } => {
            v.visit_expr(a, *lhs);
            v.visit_expr(a, *rhs);
        }
        ExpressionKind::Assign {
            target,
            value,
        }
        | ExpressionKind::AssignOp {
            target,
            value,
            ..
        } => {
            v.visit_expr(a, *target);
            v.visit_expr(a, *value);
        }
        ExpressionKind::Call {
            callee,
            args,
        } => {
            v.visit_expr(a, *callee);
            for &arg in args {
                v.visit_expr(a, arg);
            }
        }
        ExpressionKind::MethodCall {
            receiver,
            generic_args,
            args,
            ..
        } => {
            v.visit_expr(a, *receiver);
            walk_generic_args(v, a, generic_args);
            for &arg in args {
                v.visit_expr(a, arg);
            }
        }
        ExpressionKind::Field { base, .. } => v.visit_expr(a, *base),
        ExpressionKind::Index {
            base,
            index,
        } => {
            v.visit_expr(a, *base);
            v.visit_expr(a, *index);
        }
        ExpressionKind::Cast { expr, ty } => {
            v.visit_expr(a, *expr);
            v.visit_type_ref(a, *ty);
        }
        ExpressionKind::StructDeclration {
            path,
            fields,
        } => {
            v.visit_path(a, *path);
            for f in fields {
                v.visit_expr(a, f.value);
            }
        }
        ExpressionKind::Array(elems) | ExpressionKind::Tuple(elems) => {
            for &e in elems {
                v.visit_expr(a, e);
            }
        }
        ExpressionKind::ArrayRepeat { value, len } => {
            v.visit_expr(a, *value);
            v.visit_expr(a, *len);
        }
        ExpressionKind::Block(block) => {
            for &s in &block.stmts {
                v.visit_stmt(a, s);
            }
            if let Some(t) = block.tail {
                v.visit_expr(a, t);
            }
        }
        ExpressionKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            v.visit_expr(a, *cond);
            v.visit_expr(a, *then_branch);
            if let Some(e) = else_branch {
                v.visit_expr(a, *e);
            }
        }
        ExpressionKind::While { cond, body } => {
            v.visit_expr(a, *cond);
            v.visit_expr(a, *body);
        }
        ExpressionKind::Loop { body } => v.visit_expr(a, *body),
        ExpressionKind::Match {
            scrutinee,
            arms,
        } => {
            v.visit_expr(a, *scrutinee);
            for arm in arms {
                v.visit_pat(a, arm.pat);
                if let Some(g) = arm.guard {
                    v.visit_expr(a, g);
                }
                v.visit_expr(a, arm.body);
            }
        }
        ExpressionKind::Break(e) | ExpressionKind::Return(e) => {
            if let Some(e) = e {
                v.visit_expr(a, *e);
            }
        }
    }
}
