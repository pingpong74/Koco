use super::{GenericArg, Idx, Item, ParserType, Path, Pattern};
use koco_span::{Span, Symbol};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Literal {
    Int(u64),
    Float(f64),
    Bool(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl BinaryOp {
    pub fn binding_power(self) -> (u8, u8) {
        use BinaryOp::*;
        match self {
            Mul | Div | Rem => (23, 24),
            Add | Sub => (21, 22),
            Shl | Shr => (19, 20),
            BitAnd => (17, 18),
            BitXor => (15, 16),
            BitOr => (13, 14),
            Eq | Ne | Lt | Gt | Le | Ge => (11, 12),
            And => (9, 10),
            Or => (7, 8),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldName {
    Named(Symbol),
    Index(u32),
}

#[derive(Clone, Debug)]
pub enum ExpressionKind {
    Literal(Literal),
    Path(Idx<Path>),
    Unary {
        op: UnaryOp,
        expr: Idx<Expression>,
    },
    Binary {
        op: BinaryOp,
        lhs: Idx<Expression>,
        rhs: Idx<Expression>,
    },
    Assign {
        target: Idx<Expression>,
        value: Idx<Expression>,
    },
    AssignOp {
        op: BinaryOp,
        target: Idx<Expression>,
        value: Idx<Expression>,
    },
    Ref {
        mutable: bool,
        expr: Idx<Expression>,
    },
    Call {
        callee: Idx<Expression>,
        args: Vec<Idx<Expression>>,
    },
    MethodCall {
        receiver: Idx<Expression>,
        method: Symbol,
        generic_args: Vec<GenericArg>,
        args: Vec<Idx<Expression>>,
    },
    Field {
        base: Idx<Expression>,
        field: FieldName,
    },
    Index {
        base: Idx<Expression>,
        index: Idx<Expression>,
    },
    Cast {
        expr: Idx<Expression>,
        ty: Idx<ParserType>,
    },
    StructDeclration {
        path: Idx<Path>,
        fields: Vec<FieldInit>,
    },
    Array(Vec<Idx<Expression>>),
    ArrayRepeat {
        value: Idx<Expression>,
        len: Idx<Expression>,
    },
    Tuple(Vec<Idx<Expression>>),
    Block(Block),
    If {
        cond: Idx<Expression>,
        then_branch: Idx<Expression>,
        else_branch: Option<Idx<Expression>>,
    },
    While {
        cond: Idx<Expression>,
        body: Idx<Expression>,
    },
    Loop {
        body: Idx<Expression>,
    },
    Match {
        scrutinee: Idx<Expression>,
        arms: Vec<MatchArm>,
    },
    Break(Option<Idx<Expression>>),
    Continue,
    Return(Option<Idx<Expression>>),
}

#[derive(Clone, Debug)]
pub struct FieldInit {
    pub name: Symbol,
    pub value: Idx<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pat: Idx<Pattern>,
    pub guard: Option<Idx<Expression>>,
    pub body: Idx<Expression>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Idx<Statement>>,
    pub tail: Option<Idx<Expression>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum StatementKind {
    Let {
        pat: Idx<Pattern>,
        ty: Option<Idx<ParserType>>,
        init: Option<Idx<Expression>>,
    },
    Expr {
        expr: Idx<Expression>,
        semi: bool,
    },
    Item(Idx<Item>),
}
