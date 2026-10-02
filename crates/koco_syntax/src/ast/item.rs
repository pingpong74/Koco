use super::{Expression, Idx, ParserType, Path, Pattern};
use koco_span::{Span, Symbol};

#[derive(Clone, Debug)]
pub struct Attribute {
    pub name: Symbol,
    pub args: Vec<AttributeArg>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct AttributeArg {
    pub key: Option<Symbol>,
    pub value: AttrValue,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrValue {
    Ident(Symbol),
    Int(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Private,
    Public,
    Crate,
    Super,
}

// the top level for the given file
#[derive(Clone, Debug)]
pub struct Item {
    pub kind: ItemKind,
    pub vis: Visibility,
    pub attrs: Vec<Attribute>,
    pub span: Span,
}

impl Item {
    pub fn name(&self) -> Option<Symbol> {
        match &self.kind {
            ItemKind::Fn(f) => Some(f.name),
            ItemKind::Struct(s) => Some(s.name),
            ItemKind::Enum(e) => Some(e.name),
            ItemKind::Trait(t) => Some(t.name),
            ItemKind::Mod(m) => Some(m.name),
            ItemKind::Const(c) => Some(c.name),
            ItemKind::Static(s) => Some(s.name),
            ItemKind::TypeAlias(t) => Some(t.name),
            ItemKind::Impl(_) | ItemKind::Use(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ItemKind {
    Fn(FnDef),
    Struct(StructDef),
    Enum(EnumDef),
    Trait(TraitDef),
    Impl(ImplDef),
    Mod(ModDef),
    Use(UseTree),
    Const(ConstDef),
    Static(StaticDef),
    TypeAlias(TypeAliasDef),
}

#[derive(Clone, Debug, Default)]
pub struct Generics {
    pub params: Vec<Idx<GenericParam>>,
}

#[derive(Clone, Debug)]
pub struct GenericParam {
    pub name: Symbol,
    pub kind: GenericParamKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum GenericParamKind {
    Type {
        bounds: Vec<Idx<Path>>,
    },
    Const {
        ty: Idx<ParserType>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamMode {
    Value,
    RefShared,
    RefMut,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub pat: Idx<Pattern>,
    pub mode: ParamMode,
    pub ty: Idx<ParserType>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub struct SelfParam {
    pub pat: Idx<Pattern>,
    pub mode: ParamMode,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDef {
    pub name: Symbol,
    pub name_span: Span,
    pub generics: Generics,
    pub self_param: Option<SelfParam>,
    pub params: Vec<Param>,
    pub ret_ty: Option<Idx<ParserType>>,
    pub body: Option<Idx<Expression>>,
}

#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: Option<Symbol>,
    pub ty: Idx<ParserType>,
    pub vis: Visibility,
    pub attrs: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Fields {
    Named(Vec<FieldDef>),
    Tuple(Vec<FieldDef>),
    Unit,
}

impl Fields {
    pub fn defs(&self) -> &[FieldDef] {
        match self {
            Fields::Named(f) | Fields::Tuple(f) => f,
            Fields::Unit => &[],
        }
    }
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub name: Symbol,
    pub generics: Generics,
    pub fields: Fields,
}

#[derive(Clone, Debug)]
pub struct Variant {
    pub name: Symbol,
    pub fields: Fields,
    pub attrs: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub name: Symbol,
    pub generics: Generics,
    pub variants: Vec<Idx<Variant>>,
}

#[derive(Clone, Debug)]
pub struct TraitDef {
    pub name: Symbol,
    pub generics: Generics,
    pub supertraits: Vec<Idx<Path>>,
    pub items: Vec<Idx<Item>>,
}

#[derive(Clone, Debug)]
pub struct ImplDef {
    pub generics: Generics,
    pub trait_ref: Option<Idx<Path>>,
    pub self_ty: Idx<ParserType>,
    pub items: Vec<Idx<Item>>,
}

#[derive(Clone, Debug)]
pub struct ModDef {
    pub name: Symbol,
    pub kind: ModKind,
}

#[derive(Clone, Debug)]
pub enum ModKind {
    Inline(Vec<Idx<Item>>),
    Outline,
}

#[derive(Clone, Debug)]
pub struct UseTree {
    pub prefix: Vec<UseSegment>,
    pub kind: UseTreeKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug)]
pub struct UseSegment {
    pub ident: Symbol,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum UseTreeKind {
    Simple {
        rename: Option<Symbol>,
    },
    Glob,
    Nested(Vec<UseTree>),
}

#[derive(Clone, Debug)]
pub struct ConstDef {
    pub name: Symbol,
    pub ty: Idx<ParserType>,
    // use None only inside traits
    pub value: Option<Idx<Expression>>,
}

#[derive(Clone, Debug)]
pub struct StaticDef {
    pub name: Symbol,
    pub mutable: bool,
    pub ty: Idx<ParserType>,
    pub init: Option<Idx<Expression>>,
}

#[derive(Clone, Debug)]
pub struct TypeAliasDef {
    pub name: Symbol,
    pub generics: Generics,
    pub ty: Idx<ParserType>,
}
