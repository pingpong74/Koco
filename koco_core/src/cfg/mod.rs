mod builder;

pub use builder::*;

use crate::*;

pub type BlockId = u32;
pub type ValueId = u32;

#[derive(Debug, Clone)]
pub struct CfgModule {
    pub functions: Vec<CfgFunction>,
    pub descriptors: Vec<CfgDescriptor>,
}

impl CfgModule {
    pub fn dump(&self) {
        for desc in &self.descriptors {
            println!("@desc {}({}) : {} {}", desc.name, desc.id, fmt_storage(&desc.storage), fmt_type(&desc.ty));
            if let Some(b) = &desc.binding {
                println!("  binding = set={} binding={}", b.group, b.binding);
            }
        }
        for func in &self.functions {
            func.dump();
        }
    }
}

#[derive(Debug, Clone)]
pub struct CfgFunction {
    pub id: FunctionId,
    pub name: String,
    pub signature: FunctionType,
    pub parameters: Vec<CfgParameter>,
    pub blocks: Vec<BasicBlock>,
    pub entry_block: BlockId,
    pub stage: ShaderStage,
}

impl CfgFunction {
    pub fn dump(&self) {
        let params: Vec<String> = self
            .parameters
            .iter()
            .map(|p| format!("%{}: {}", p.id, fmt_type(&p.ty)))
            .collect();
        let ret = fmt_type(&self.signature.return_type);
        let stage_str = match self.stage {
            ShaderStage::Vertex => " #[vertex]",
            ShaderStage::Fragment => " #[fragment]",
            ShaderStage::Compute{..} => " #[compute]",
            ShaderStage::None => "",
        };
        println!("\nfn {}({}) -> {}{} {{", self.name, params.join(", "), ret, stage_str);

        for block in &self.blocks {
            let label = if block.id == self.entry_block { " (entry)" } else { "" };
            println!("  --- bb{}{} ---", block.id, label);
            for inst in &block.instructions {
                println!("    {} = {}", fmt_val(inst.id), fmt_op(&inst.op, &inst.ty));
            }
            println!("    {}", fmt_terminator(&block.terminator));
        }

        println!("}}");
    }
}

#[derive(Debug, Clone)]
pub struct FunctionType {
    pub param_types: Vec<Type>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct CfgParameter {
    pub id: ValueId,
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub id: ValueId,
    pub ty: Type,
    pub op: Op,
}

#[derive(Debug, Clone)]
pub enum Op {
    Constant(Literal),
    Variable(StorageClass),
    Load(ValueId),
    Store(ValueId, ValueId),
    AccessChain {
        base: ValueId,
        indices: Vec<ValueId>,
    },
    Binary(BinaryOp, ValueId, ValueId, Type),
    Unary(UnaryOp, ValueId),
    Call(FunctionId, Vec<ValueId>),
    CompositeConstruct(Vec<ValueId>),
    CompositeExtract {
        composite: ValueId,
        index: u32,
    },
    CompositeInsert {
        composite: ValueId,
        value: ValueId,
        index: u32,
    },
    Phi(Vec<(ValueId, BlockId)>),
    Cast {
        from_type: Type,
        to_type: Type,
        value: ValueId,
    },
    EnumConstruct {
        variant_index: u32,
        payload: Option<ValueId>,
    },
    SelectionMerge(BlockId),
    LoopMerge {
        merge: BlockId,
        continue_block: BlockId,
    },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Branch {
        target: BlockId,
    },
    BranchCond {
        condition: ValueId,
        true_target: BlockId,
        false_target: BlockId,
    },
    Return {
        value: Option<ValueId>,
    },
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct CfgDescriptor {
    pub id: ValueId,
    pub name: String,
    pub ty: Type,
    pub storage: StorageClass,
    pub binding: Option<Binding>,
}

// --- formatting helpers ---

fn fmt_type(ty: &Type) -> String {
    match ty {
        Type::Void => "void".to_string(),
        Type::Scalar(s) => fmt_scalar(s).to_string(),
        Type::Vector(s, n) => format!("{}vec{}", fmt_scalar(s), n),
        Type::Array(inner, n) => format!("[{}; {}]", fmt_type(inner), n),
        Type::Struct(name) => name.clone(),
        Type::Enum(name) => name.clone(),
        Type::Pointer(inner, _) => format!("ptr<{}>", fmt_type(inner)),
    }
}

fn fmt_scalar(s: &ScalarType) -> &'static str {
    match s {
        ScalarType::Bool => "bool",
        ScalarType::I32 => "i32",
        ScalarType::U32 => "u32",
        ScalarType::F32 => "f32",
        ScalarType::F64 => "f64",
    }
}

fn fmt_storage(s: &StorageClass) -> &'static str {
    match s {
        StorageClass::Function => "function",
        StorageClass::Descriptor => "descriptor",
        StorageClass::PushConstant => "push_constant",
    }
}

fn fmt_val(id: ValueId) -> String {
    format!("%{}", id)
}

fn fmt_op(op: &Op, ty: &Type) -> String {
    match op {
        Op::Constant(lit) => format!("const {}", fmt_literal(lit)),
        Op::Variable(sc) => format!("var({})", fmt_storage(sc)),
        Op::Load(ptr) => format!("load({})", fmt_val(*ptr)),
        Op::Store(ptr, val) => format!("store({}, {})", fmt_val(*ptr), fmt_val(*val)),
        Op::AccessChain {
            base,
            indices,
        } => {
            let idxs: Vec<String> = indices.iter().map(|i| fmt_val(*i)).collect();
            format!("access_chain({}, [{}])", fmt_val(*base), idxs.join(", "))
        }
        Op::Binary(op, l, r, _) => format!("{}.{}.{}", fmt_val(*l), fmt_binary(op), fmt_val(*r)),
        Op::Unary(op, v) => format!("{}.{}", fmt_unary(op), fmt_val(*v)),
        Op::Call(func, args) => {
            let a: Vec<String> = args.iter().map(|a| fmt_val(*a)).collect();
            format!("call(fn{} [{}])", func.id, a.join(", "))
        }
        Op::CompositeConstruct(args) => {
            let a: Vec<String> = args.iter().map(|a| fmt_val(*a)).collect();
            format!("construct([{}])", a.join(", "))
        }
        Op::CompositeExtract {
            composite,
            index,
        } => {
            format!("extract({}, {})", fmt_val(*composite), index)
        }
        Op::CompositeInsert {
            composite,
            value,
            index,
        } => {
            format!("insert({}, {}, {})", fmt_val(*composite), fmt_val(*value), index)
        }
        Op::Phi(incoming) => {
            let inc: Vec<String> = incoming
                .iter()
                .map(|(v, b)| format!("({}, bb{})", fmt_val(*v), b))
                .collect();
            format!("phi({})", inc.join(", "))
        }
        Op::Cast {
            from_type: _,
            to_type: _,
            value,
        } => {
            format!("cast({} -> {})", fmt_val(*value), fmt_type(ty))
        }
        Op::EnumConstruct {
            variant_index,
            payload,
        } => {
            let p = payload.map(|v| fmt_val(v)).unwrap_or_else(|| "void".into());
            format!("enum({}, {})", variant_index, p)
        }
        Op::SelectionMerge(merge) => format!("selection_merge bb{}", merge),
        Op::LoopMerge {
            merge,
            continue_block,
        } => {
            format!("loop_merge bb{} continue bb{}", merge, continue_block)
        }
    }
}

fn fmt_terminator(t: &Terminator) -> String {
    match t {
        Terminator::Branch { target } => format!("br bb{}", target),
        Terminator::BranchCond {
            condition,
            true_target,
            false_target,
        } => {
            format!("br({}) bb{} bb{}", fmt_val(*condition), true_target, false_target)
        }
        Terminator::Return { value } => {
            if let Some(v) = value {
                format!("ret {}", fmt_val(*v))
            } else {
                "ret void".into()
            }
        }
        Terminator::Unreachable => "unreachable".into(),
    }
}

fn fmt_literal(lit: &Literal) -> String {
    match lit {
        Literal::Bool(b) => b.to_string(),
        Literal::Int(i) => i.to_string(),
        Literal::Uint(u) => format!("{}u", u),
        Literal::Float(f) => format!("{}", f),
    }
}

fn fmt_binary(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "add",
        BinaryOp::Subtract => "sub",
        BinaryOp::Multiply => "mul",
        BinaryOp::Divide => "div",
        BinaryOp::Remainder => "rem",
        BinaryOp::IsEqual => "eq",
        BinaryOp::IsNotEqual => "neq",
        BinaryOp::LessThan => "lt",
        BinaryOp::LessEqual => "le",
        BinaryOp::GreaterThan => "gt",
        BinaryOp::GreaterEqual => "ge",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
        BinaryOp::BitAnd => "band",
        BinaryOp::BitOr => "bor",
        BinaryOp::BitXor => "bxor",
        BinaryOp::BitShiftL => "shl",
        BinaryOp::BitShiftR => "shr",
    }
}

fn fmt_unary(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Negate => "neg",
        UnaryOp::Not => "not",
    }
}
