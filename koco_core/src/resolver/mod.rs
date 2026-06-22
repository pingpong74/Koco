mod resolver;

use std::collections::HashMap;

use crate::*;
pub use resolver::*;

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub ty: Type,
    pub location: Option<u32>,
    pub builtin: Option<BuiltInVar>,
}

pub type StructFields = Vec<FieldInfo>;
pub type StructRegistry = HashMap<String, StructFields>;

pub type EnumVariants = Vec<(String, Type)>;
pub type EnumRegistry = HashMap<String, EnumVariants>;

pub struct ResolvedSyntaxTree {
    pub functions: Vec<ResolvedFunction>,
    pub descriptors: Vec<ResolvedDescriptor>,
    pub globals: Vec<ResolvedLocalVariable>,
    pub structs: StructRegistry,
    pub enums: EnumRegistry,
}

// define the variable ids and function id
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuiltInVar {
    VertexId,
    Position,
    FragCoord,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum VariableId {
    Local(u32),
    Global(u32),
    Builtin(BuiltInVar),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionId {
    pub id: u32,
}

// functions
#[derive(Debug, Clone)]
pub struct ResolvedFunction {
    pub id: FunctionId,
    pub debug_name: String,
    pub params: Vec<ResolvedParameter>,
    pub return_type: Type,
    pub body: ResolvedScope,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone)]
pub struct ResolvedParameter {
    pub id: VariableId,
    pub debug_name: String,
    pub var_type: Type,
}

// scope
#[derive(Debug, Clone)]
pub struct ResolvedScope {
    pub statements: Vec<ResolvedStatement>,
}

// statements
#[derive(Debug, Clone)]
pub enum ResolvedStatement {
    Declaration(ResolvedLocalVariable),
    Assign {
        target: ResolvedExpression,
        value: ResolvedExpression,
    },
    If {
        scopes: Vec<ResolvedScope>,
        conditions: Vec<ResolvedExpression>,
    },
    Loop {
        scope: ResolvedScope,
    },
    While {
        condition: ResolvedExpression,
        scope: ResolvedScope,
    },
    FunctionCall(ResolvedExpression),
    Return(Option<ResolvedExpression>),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct ResolvedLocalVariable {
    pub mutable: bool,
    pub id: VariableId,
    pub debug_name: String,
    pub var_type: Option<Type>,
    pub init: Option<ResolvedExpression>,
}

// expression
#[derive(Debug, Clone)]
pub enum ResolvedExpression {
    Literal(Literal),
    Variable(VariableId), // variable
    Unary {
        op: UnaryOp,
        expr: Box<ResolvedExpression>,
    },
    Binary {
        op: BinaryOp,
        left: Box<ResolvedExpression>,
        right: Box<ResolvedExpression>,
    },
    Call {
        function: FunctionId,
        args: Vec<ResolvedExpression>,
    },

    StructLiteral {
        struct_name: String,
        fields: Vec<(String, ResolvedExpression)>,
    },
    Member {
        base: Box<ResolvedExpression>,
        field: String,
    },
    Index {
        base: Box<ResolvedExpression>,
        index: Box<ResolvedExpression>,
    },
    Cast {
        ty: Type,
        expr: Box<ResolvedExpression>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        variant_index: u32,
        payload: Option<Box<ResolvedExpression>>,
    },
    ArrayLiteral(ResolvedArrayLiteral),
}

#[derive(Debug, Clone)]
pub enum ResolvedArrayLiteral {
    Repeat {
        value: Box<ResolvedExpression>,
        count: u32,
    },
    Normal {
        values: Vec<ResolvedExpression>,
    },
}

// descriptors
#[derive(Debug, Clone)]
pub struct ResolvedDescriptor {
    pub id: VariableId,
    pub debug_name: String,
    pub ty: Type,
    pub storage: StorageClass,
    pub binding: Option<Binding>,
}
