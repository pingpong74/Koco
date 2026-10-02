use super::{Expression, Idx, ParserType};
use koco_span::{Span, Symbol};

#[derive(Clone, Debug)]
pub struct Path {
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

impl Path {
    pub fn as_ident(&self) -> Option<Symbol> {
        match self.segments.as_slice() {
            [seg] if seg.args.is_empty() => Some(seg.ident),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PathSegment {
    pub ident: Symbol,
    pub args: Vec<GenericArg>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum GenericArg {
    Type(Idx<ParserType>),
    Const(ConstArg),
    AssocBinding {
        name: Symbol,
        ty: Idx<ParserType>,
        span: Span,
    },
}

/// A compile-time value used as a generic argument or an array length.
#[derive(Clone, Debug)]
pub enum ConstArg {
    Literal(super::Literal, Span),
    /// A named constant or const generic parameter.
    Path(Idx<Path>),
    /// `{ expr }`; the expression is a block expression.
    Block(Idx<Expression>),
}
