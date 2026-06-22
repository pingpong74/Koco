// descriptors
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StorageClass {
    Function,
    Descriptor,
    PushConstant,
}

#[derive(Debug, Clone, Copy)]
pub struct Binding {
    pub group: u32,
    pub binding: u32,
}

// function attribute
#[derive(Debug, Clone, Copy)]
pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute {
        workgroup_size: [u32; 3],
    },
    None,
}

// types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScalarType {
    Bool,
    I32,
    U32,
    F32,
    F64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Void,
    Scalar(ScalarType),
    Vector(ScalarType, u32),
    Array(Box<Type>, u32),
    Struct(String),
    Enum(String),
    Pointer(Box<Type>, StorageClass),
}

// litterals
#[derive(Debug, Clone, Copy)]
pub enum Literal {
    Bool(bool),
    Int(i64),
    Uint(u64),
    Float(f64),
}

// operations
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Negate,
    Not,
}

impl UnaryOp {
    pub fn prefix_binding_power(&self) -> u8 {
        return 99;
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    IsEqual,
    IsNotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    BitShiftL,
    BitShiftR,
}

impl BinaryOp {
    pub fn get_binding_power(&self) -> (u8, u8) {
        match self {
            Self::Or => (1, 2),
            Self::And => (3, 4),
            Self::BitOr => (5, 6),
            Self::BitXor => (7, 8),
            Self::BitAnd => (9, 10),
            Self::IsEqual | Self::IsNotEqual => (11, 12),
            Self::LessThan | Self::LessEqual | Self::GreaterThan | Self::GreaterEqual => (13, 14),
            Self::BitShiftL | Self::BitShiftR => (15, 16),
            Self::Add | Self::Subtract => (17, 18),
            Self::Multiply | Self::Divide | Self::Remainder => (19, 20),
        }
    }
}
