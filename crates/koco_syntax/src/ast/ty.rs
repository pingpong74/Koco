use super::{ConstArg, Idx, Path};
use koco_span::Span;

#[derive(Clone, Debug)]
pub struct ParserType {
    pub kind: ParserTypeKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum ParserTypeKind {
    Path(Idx<Path>),
    Array(Idx<ParserType>, ConstArg),
    Tuple(Vec<Idx<ParserType>>),
}
