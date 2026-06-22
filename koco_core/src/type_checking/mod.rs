mod type_checker;

use crate::resolver::{EnumRegistry, StructRegistry};
use crate::*;

pub use type_checker::{TypeChecker, TypeError};

#[derive(Default)]
pub struct TypedSyntaxTree {
    pub functions: Vec<TypedFunction>,
    pub descriptors: Vec<ResolvedDescriptor>,
    pub globals: Vec<TypedLocalVariable>,
    pub structs: StructRegistry,
    pub enums: EnumRegistry,
}

#[derive(Debug, Clone)]
pub struct TypedFunction {
    pub id: FunctionId,
    pub debug_name: String,
    pub params: Vec<TypedParameter>,
    pub return_type: Type,
    pub body: TypedScope,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone)]
pub struct TypedParameter {
    pub id: VariableId,
    pub debug_name: String,
    pub var_type: Type,
}

#[derive(Debug, Clone)]
pub struct TypedScope {
    pub statements: Vec<TypedStatement>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub enum TypedStatement {
    Declaration(TypedLocalVariable),
    Assign {
        target: TypedExpression,
        value: TypedExpression,
    },
    If {
        scopes: Vec<TypedScope>,
        conditions: Vec<TypedExpression>,
    },
    Loop {
        scope: TypedScope,
    },
    While {
        condition: TypedExpression,
        scope: TypedScope,
    },
    FunctionCall(TypedExpression),
    Return(Option<TypedExpression>),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct TypedLocalVariable {
    pub mutable: bool,
    pub id: VariableId,
    pub debug_name: String,
    pub var_type: Type,
    pub init: Option<TypedExpression>,
}

#[derive(Debug, Clone)]
pub struct TypedExpression {
    pub ty: Type,
    pub kind: ExpressionKind,
}

#[derive(Debug, Clone)]
pub enum ExpressionKind {
    Literal(Literal),
    Variable(VariableId), // variable
    Unary {
        op: UnaryOp,
        expr: Box<TypedExpression>,
    },
    Binary {
        op: BinaryOp,
        left: Box<TypedExpression>,
        right: Box<TypedExpression>,
    },
    Call {
        function: FunctionId,
        args: Vec<TypedExpression>,
    },

    StructLiteral {
        struct_name: String,
        fields: Vec<(String, TypedExpression)>,
    },
    Member {
        base: Box<TypedExpression>,
        field: String,
        field_index: u32,
    },
    Index {
        base: Box<TypedExpression>,
        index: Box<TypedExpression>,
    },
    Cast {
        ty: Type,
        expr: Box<TypedExpression>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        variant_index: u32,
        payload: Option<Box<TypedExpression>>,
    },
    ArrayLiteral {
        values: Vec<TypedExpression>,
    },
}
