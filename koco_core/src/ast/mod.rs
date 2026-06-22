mod expr;
mod func;
mod item;
mod parser;
mod stmt;

pub use parser::*;

use crate::*;

pub struct SyntaxTree {
    pub descriptors: Vec<Descriptors>,
    pub functions: Vec<Function>,
    pub global_variables: Vec<Statement>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
}

#[derive(Debug, Clone)]
pub enum ParserType {
    Single(String),
    Array(Box<ParserType>, u32),
    Void,
}

impl From<ParserType> for Type {
    fn from(ty: ParserType) -> Self {
        match ty {
            ParserType::Void => Type::Void,
            ParserType::Single(name) => Self::from_name(&name),
            ParserType::Array(inner, len) => Type::Array(Box::new(Type::from(*inner)), len),
        }
    }
}

impl Type {
    pub fn from_name(name: &str) -> Self {
        match name {
            "bool" => Type::Scalar(ScalarType::Bool),
            "i32" => Type::Scalar(ScalarType::I32),
            "u32" => Type::Scalar(ScalarType::U32),
            "f32" => Type::Scalar(ScalarType::F32),
            "f64" => Type::Scalar(ScalarType::F64),
            "float" => Type::Scalar(ScalarType::F32),
            "int" => Type::Scalar(ScalarType::I32),
            "uint" => Type::Scalar(ScalarType::U32),
            "float2" => Type::Vector(ScalarType::F32, 2),
            "float3" => Type::Vector(ScalarType::F32, 3),
            "float4" => Type::Vector(ScalarType::F32, 4),
            "int2" => Type::Vector(ScalarType::I32, 2),
            "int3" => Type::Vector(ScalarType::I32, 3),
            "int4" => Type::Vector(ScalarType::I32, 4),
            "uint2" => Type::Vector(ScalarType::U32, 2),
            "uint3" => Type::Vector(ScalarType::U32, 3),
            "uint4" => Type::Vector(ScalarType::U32, 4),
            "bool2" => Type::Vector(ScalarType::Bool, 2),
            "bool3" => Type::Vector(ScalarType::Bool, 3),
            "bool4" => Type::Vector(ScalarType::Bool, 4),
            _ => Type::Struct(name.to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FieldAttrs {
    pub location: Option<u32>,
    pub builtin: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, ParserType, FieldAttrs)>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub kinds: Vec<(String, ParserType)>,
}

#[derive(Debug, Clone)]
pub struct LocalVariable {
    pub mutable: bool,
    pub name: String,
    pub var_type: Option<ParserType>,
    pub init: Option<Expression>,
}

#[derive(Debug, Clone)]
pub struct Scope {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    /// let x: T = expr;
    /// let mut x: T = expr;
    Declaration(LocalVariable),
    /// x = expr;
    Assign {
        target: Expression,
        value: Expression,
    },
    If {
        scopes: Vec<Scope>,
        conditions: Vec<Expression>,
    },
    Loop {
        scope: Scope,
    },
    While {
        condition: Expression,
        scope: Scope,
    },
    FunctionCall(Expression),
    Return(Option<Expression>),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub enum Expression {
    // stores the numeric literals
    Literal(Literal),
    StructLiteral {
        name: String,
        fields: Vec<(String, Expression)>,
    },
    EnumLiteral {
        enum_name: String,
        variant: String,
        payload: Option<Box<Expression>>,
    },
    Variable(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expression>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Call {
        function: String,
        args: Vec<Expression>,
    },
    Member {
        object: Box<Expression>,
        field: String,
    },
    Index {
        object: Box<Expression>,
        index: Box<Expression>,
    },
    Cast {
        ty: ParserType,
        expr: Box<Expression>,
    },
    ArrayDeclaration(ArrayDeclaration),
}

#[derive(Debug, Clone)]
pub enum ArrayDeclaration {
    Repeat {
        value: Box<Expression>,
        len: Box<Expression>,
    },
    Normal {
        values: Vec<Expression>,
    },
}

// functions
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Parameter>,
    pub return_type: ParserType,
    pub body: Scope,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub var_type: ParserType,
}

// descriptors
#[derive(Debug, Clone)]
pub struct Descriptors {
    pub name: String,
    pub ty: ParserType,
    pub storage: StorageClass,
    pub binding: Option<Binding>,
}
