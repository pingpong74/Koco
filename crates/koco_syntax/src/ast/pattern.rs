use super::{Idx, Literal, Path};
use koco_span::{Span, Symbol};

/// A pattern: matches a value and binds names from it (`Some(x)`, `1`, `_`,
/// `Point { y: py, .. }`).
///
/// This node *is* a local's definition — a `Binding` pattern is what name resolution resolves a
/// local to. Parameters and `self` are patterns too, so `let`, params, `self` and match arms all
/// share one arena and one side table.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum PatternKind {
    Wildcard,
    Binding {
        name: Symbol,
        mutable: bool,
        sub: Option<Idx<Pattern>>,
    },
    Literal {
        value: Literal,
        negative: bool,
    },
    Path(Idx<Path>),
    TupleStruct {
        path: Idx<Path>,
        fields: Vec<Idx<Pattern>>,
    },
    Struct {
        path: Idx<Path>,
        fields: Vec<FieldPattern>,
        rest: bool,
    },
    Tuple(Vec<Idx<Pattern>>),
    Or(Vec<Idx<Pattern>>),
}

#[derive(Clone, Debug)]
pub struct FieldPattern {
    pub name: Symbol,
    pub pat: Idx<Pattern>,
    pub span: Span,
}
