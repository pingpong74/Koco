use std::collections::HashMap;

use rspirv::binary::Assemble;
use rspirv::dr::{InsertPoint, Instruction, Operand};
use rspirv::spirv;

use crate::cfg::*;
use crate::resolver::{EnumRegistry, FieldInfo, FunctionId, StructRegistry};
use crate::*;

pub struct Codegen {
    builder: rspirv::dr::Builder,
    type_cache: HashMap<String, spirv::Word>,
    val_cache: HashMap<ValueId, spirv::Word>,
    block_cache: HashMap<BlockId, spirv::Word>,
    func_cache: HashMap<FunctionId, spirv::Word>,
    struct_registry: StructRegistry,
    enum_registry: EnumRegistry,
}

impl Codegen {
    pub fn new() -> Self {
        return Self {
            builder: rspirv::dr::Builder::new(),
            type_cache: HashMap::new(),
            val_cache: HashMap::new(),
            block_cache: HashMap::new(),
            func_cache: HashMap::new(),
            struct_registry: HashMap::new(),
            enum_registry: HashMap::new(),
        };
    }

    pub fn with_structs(mut self, structs: StructRegistry) -> Self {
        self.struct_registry = structs;
        return self;
    }

    pub fn with_enums(mut self, enums: EnumRegistry) -> Self {
        self.enum_registry = enums;
        return self;
    }

    pub fn codegen(&mut self, module: &CfgModule) -> Vec<u32> {
        self.builder.set_version(1, 0);
        self.builder.capability(spirv::Capability::Shader);
        self.builder.capability(spirv::Capability::Linkage);
        self.builder.memory_model(
            spirv::AddressingModel::Logical,
            spirv::MemoryModel::GLSL450,
        );

        // Pre-assign function IDs so descriptors and calls can reference them
        for func in &module.functions {
            let id = self.builder.id();
            self.func_cache.insert(func.id, id);
        }

        for desc in &module.descriptors {
            self.gen_descriptor(desc);
        }

        for func in &module.functions {
            self.gen_function(func);
        }

        let bound = self.builder.id();
        self.builder.module_mut().header.as_mut().unwrap().bound = bound;
        let mut output = Vec::new();
        self.builder.module_ref().assemble_into(&mut output);
        return output;
    }

    // --- type system ---

    fn type_key(ty: &Type) -> String {
        return match ty {
            Type::Void => "void".into(),
            Type::Scalar(s) => format!("scalar_{:?}", s),
            Type::Vector(s, n) => format!("vec_{:?}_{}", s, n),
            Type::Array(inner, n) => format!("arr_{}_{}", Self::type_key(inner), n),
            Type::Struct(name) => format!("struct_{}", name),
            Type::Enum(name) => format!("enum_{}", name),
            Type::Pointer(inner, sc) => format!("ptr_{:?}_{}", sc, Self::type_key(inner)),
        };
    }

    fn get_type(&mut self, ty: &Type) -> spirv::Word {
        let key = Self::type_key(ty);
        if let Some(&id) = self.type_cache.get(&key) {
            return id;
        }
        let id = self.emit_type(ty);
        self.type_cache.insert(key, id);
        return id;
    }

    fn emit_type(&mut self, ty: &Type) -> spirv::Word {
        return match ty {
            Type::Void => self.builder.type_void(),
            Type::Scalar(s) => self.emit_scalar_type(s),
            Type::Vector(s, n) => {
                let comp = self.get_type(&Type::Scalar(*s));
                self.builder.type_vector(comp, *n)
            }
            Type::Array(inner, len) => {
                let elem = self.get_type(inner);
                let len_const = self.get_uint_const(*len);
                self.builder.type_array(elem, len_const)
            }
            Type::Struct(name) => {
                let id = self.builder.id();
                let sfields = self.struct_registry.get(name).cloned().unwrap_or_default();
                let mut operands = vec![];
                for fi in &sfields {
                    let member_ty = self.get_type(&fi.ty);
                    operands.push(Operand::IdRef(member_ty));
                }
                for (i, fi) in sfields.iter().enumerate() {
                    self.builder.member_name(id, i as u32, fi.name.clone());
                }
                let inst = Instruction::new(
                    spirv::Op::TypeStruct,
                    None,
                    Some(id),
                    operands,
                );
                self.builder.insert_types_global_values(InsertPoint::End, inst);
                id
            }
            Type::Enum(name) => {
                let variants = self.enum_registry.get(name).cloned().unwrap_or_default();
                let has_payload = variants.iter().any(|(_, t)| *t != Type::Void);
                if has_payload {
                    let payload_ty = variants.iter().find_map(|(_, t)| if *t != Type::Void { Some(t.clone()) } else { None }).unwrap_or(Type::Void);
                    let u32_spv = self.get_type(&Type::Scalar(ScalarType::U32));
                    let payload_spv_ty = self.get_type(&payload_ty);
                    let id = self.builder.id();
                    let inst = Instruction::new(
                        spirv::Op::TypeStruct,
                        None,
                        Some(id),
                        vec![Operand::IdRef(u32_spv), Operand::IdRef(payload_spv_ty)],
                    );
                    self.builder.insert_types_global_values(InsertPoint::End, inst);
                    id
                } else {
                    self.builder.type_int(32, 0)
                }
            }
            Type::Pointer(inner, sc) => {
                let pointee = self.get_type(inner);
                self.builder.type_pointer(None, spirv_storage_class(*sc), pointee)
            }
        };
    }

    fn emit_scalar_type(&mut self, s: &ScalarType) -> spirv::Word {
        return match s {
            ScalarType::Bool => self.builder.type_bool(),
            ScalarType::I32 => self.builder.type_int(32, 1),
            ScalarType::U32 => self.builder.type_int(32, 0),
            ScalarType::F32 => self.builder.type_float(32, None),
            ScalarType::F64 => self.builder.type_float(64, None),
        };
    }

    fn get_uint_const(&mut self, val: u32) -> spirv::Word {
        let u32_ty = self.get_type(&Type::Scalar(ScalarType::U32));
        let id = self.builder.id();
        let inst = Instruction::new(
            spirv::Op::Constant,
            Some(u32_ty),
            Some(id),
            vec![Operand::LiteralBit32(val)],
        );
        self.builder.insert_types_global_values(InsertPoint::End, inst);
        return id;
    }

    // --- descriptors ---

    fn gen_descriptor(&mut self, desc: &CfgDescriptor) {
        let ptr_ty = self.get_type(&Type::Pointer(
            Box::new(desc.ty.clone()),
            desc.storage,
        ));
        let sc = spirv_storage_class(desc.storage);
        let var_id = self.builder.id();
        let inst = Instruction::new(
            spirv::Op::Variable,
            Some(ptr_ty),
            Some(var_id),
            vec![Operand::StorageClass(sc)],
        );
        self.builder.insert_types_global_values(InsertPoint::End, inst);

        self.builder.name(var_id, desc.name.clone());

        if let Some(binding) = &desc.binding {
            self.builder.decorate(var_id, spirv::Decoration::DescriptorSet, vec![Operand::LiteralBit32(binding.group)]);
            self.builder.decorate(var_id, spirv::Decoration::Binding, vec![Operand::LiteralBit32(binding.binding)]);
        }

        self.val_cache.insert(desc.id, var_id);
    }

    // --- functions ---

    fn get_output_fields(&self, func: &CfgFunction) -> Option<Vec<(usize, FieldInfo)>> {
        if matches!(func.stage, ShaderStage::None) {
            return None;
        }
        let struct_name = match &func.signature.return_type {
            Type::Struct(name) => name,
            _ => return None,
        };
        let fields = self.struct_registry.get(struct_name)?;
        let decorated: Vec<(usize, FieldInfo)> = fields
            .iter()
            .enumerate()
            .filter(|(_, fi)| fi.location.is_some() || fi.builtin.is_some())
            .map(|(i, fi)| (i, fi.clone()))
            .collect();
        if decorated.is_empty() {
            return None;
        }
        return Some(decorated);
    }

    fn gen_function(&mut self, func: &CfgFunction) {
        let output_fields = self.get_output_fields(func);
        let has_outputs = output_fields.is_some();

        let ret_ty = if has_outputs {
            self.get_type(&Type::Void)
        } else {
            self.get_type(&func.signature.return_type)
        };

        let func_ty_id = if has_outputs {
            let param_tys: Vec<spirv::Word> = func
                .signature
                .param_types
                .iter()
                .map(|t| self.get_type(t))
                .collect();
            self.builder.type_function(ret_ty, param_tys)
        } else {
            self.get_or_create_function_type(func)
        };

        let fn_id = *self.func_cache.get(&func.id).unwrap();
        self.builder
            .begin_function(ret_ty, Some(fn_id), spirv::FunctionControl::empty(), func_ty_id)
            .unwrap();
        self.val_cache.clear();
        for param in func.parameters.iter() {
            let param_ty = self.get_type(&param.ty);
            let param_id = self.builder.function_parameter(param_ty).unwrap();
            self.val_cache.insert(param.id, param_id);

            self.builder.name(param_id, param.name.clone());
        }

        self.builder.name(fn_id, func.name.clone());

        // Emit output OpVariables for decorated return struct fields
        let mut output_vars: Vec<(spirv::Word, u32, Type)> = Vec::new();
        if let Some(ref outs) = output_fields {
            for (field_idx, fi) in outs {
                let val_ty = self.get_type(&fi.ty);
                let ptr_ty = self.builder.type_pointer(None, spirv::StorageClass::Output, val_ty);
                let var_id = self.builder.id();
                let inst = Instruction::new(
                    spirv::Op::Variable,
                    Some(ptr_ty),
                    Some(var_id),
                    vec![Operand::StorageClass(spirv::StorageClass::Output)],
                );
                self.builder.insert_types_global_values(InsertPoint::End, inst);
                self.builder.name(var_id, fi.name.clone());

                if let Some(loc) = fi.location {
                    self.builder.decorate(var_id, spirv::Decoration::Location, vec![Operand::LiteralBit32(loc)]);
                }
                if let Some(bi) = fi.builtin {
                    self.builder.decorate(var_id, spirv::Decoration::BuiltIn, vec![Operand::BuiltIn(builtin_to_spirv(bi))]);
                }
                output_vars.push((var_id, *field_idx as u32, fi.ty.clone()));
            }
        }

        self.block_cache.clear();
        for block in &func.blocks {
            let label_id = self.builder.id();
            self.block_cache.insert(block.id, label_id);
        }

        for block in &func.blocks {
            let label_id = *self.block_cache.get(&block.id).unwrap();
            self.builder.begin_block(Some(label_id)).unwrap();

            for inst in &block.instructions {
                self.gen_cfg_inst(inst);
            }

            // For entry points with decorated outputs, decompose struct return
            // into per-field stores to output variables, then void return.
            if !output_vars.is_empty() {
                if let Terminator::Return { value: Some(v) } = &block.terminator {
                    let struct_spv = *self.val_cache.get(v).unwrap();
                    for (var_id, field_idx, field_ty) in &output_vars {
                        let field_spv_ty = self.get_type(field_ty);
                        let field_spv = self.builder.id();
                        let extract = Instruction::new(
                            spirv::Op::CompositeExtract,
                            Some(field_spv_ty),
                            Some(field_spv),
                            vec![Operand::IdRef(struct_spv), Operand::LiteralBit32(*field_idx)],
                        );
                        self.builder.insert_into_block(InsertPoint::End, extract).unwrap();

                        let store = Instruction::new(
                            spirv::Op::Store,
                            None,
                            None,
                            vec![Operand::IdRef(*var_id), Operand::IdRef(field_spv)],
                        );
                        self.builder.insert_into_block(InsertPoint::End, store).unwrap();
                    }
                    self.builder.ret().unwrap();
                    continue;
                }
            }

            self.gen_terminator(&block.terminator);
        }

        self.builder.end_function().unwrap();

        // Emit OpEntryPoint when the function has a shader stage and output variables
        if has_outputs {
            let exec_model = stage_to_execution_model(func.stage);
            let var_ids: Vec<spirv::Word> = output_vars.iter().map(|(id, _, _)| *id).collect();
            self.builder.entry_point(exec_model, fn_id, &func.name, var_ids);

            if matches!(func.stage, ShaderStage::Fragment) {
                self.builder.execution_mode(fn_id, spirv::ExecutionMode::OriginUpperLeft, vec![]);
            }
        }
    }

    fn get_or_create_function_type(&mut self, func: &CfgFunction) -> spirv::Word {
        let ret_ty = self.get_type(&func.signature.return_type);
        let param_tys: Vec<spirv::Word> = func
            .signature
            .param_types
            .iter()
            .map(|t| self.get_type(t))
            .collect();
        return self.builder.type_function(ret_ty, param_tys);
    }

    // --- CFG instructions -> SPIR-V ---

    fn gen_cfg_inst(&mut self, inst: &cfg::Instruction) {
        let id = self.builder.id();
        let spirv_ty = self.get_type(&inst.ty);

        match &inst.op {
            Op::Constant(lit) => {
                let const_id = self.gen_spirv_constant(&inst.ty, lit);
                self.val_cache.insert(inst.id, const_id);
            }
            Op::Variable(sc) => {
                let spv_inst = Instruction::new(
                    spirv::Op::Variable,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::StorageClass(spirv_storage_class(*sc))],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Load(ptr) => {
                let ptr_spv = *self.val_cache.get(ptr).unwrap();
                let spv_inst = Instruction::new(
                    spirv::Op::Load,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::IdRef(ptr_spv)],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Store(ptr, val) => {
                let ptr_spv = *self.val_cache.get(ptr).unwrap();
                let val_spv = *self.val_cache.get(val).unwrap();
                let spv_inst = Instruction::new(
                    spirv::Op::Store,
                    None,
                    None,
                    vec![Operand::IdRef(ptr_spv), Operand::IdRef(val_spv)],
                );
                self.insert_in_block(spv_inst);
            }
            Op::AccessChain { base, indices } => {
                let base_spv = *self.val_cache.get(base).unwrap();
                let mut operands = vec![Operand::IdRef(base_spv)];
                for idx in indices {
                    operands.push(Operand::IdRef(*self.val_cache.get(idx).unwrap()));
                }
                let spv_inst = Instruction::new(
                    spirv::Op::AccessChain,
                    Some(spirv_ty),
                    Some(id),
                    operands,
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Binary(bin_op, lhs, rhs, input_ty) => {
                let lhs_spv = *self.val_cache.get(lhs).unwrap();
                let rhs_spv = *self.val_cache.get(rhs).unwrap();
                let spv_op = binary_to_spirv(&inst.ty, input_ty, bin_op);
                let spv_inst = Instruction::new(
                    spv_op,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::IdRef(lhs_spv), Operand::IdRef(rhs_spv)],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Unary(un_op, val) => {
                let val_spv = *self.val_cache.get(val).unwrap();
                let spv_op = unary_to_spirv(un_op, &inst.ty);
                let spv_inst = Instruction::new(
                    spv_op,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::IdRef(val_spv)],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Call(func_id, args) => {
                let func_spv = *self.func_cache.get(func_id).unwrap();
                let mut operands = vec![Operand::IdRef(func_spv)];
                for arg in args {
                    operands.push(Operand::IdRef(*self.val_cache.get(arg).unwrap()));
                }
                let spv_inst = Instruction::new(
                    spirv::Op::FunctionCall,
                    Some(spirv_ty),
                    Some(id),
                    operands,
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Phi(incoming) => {
                let mut operands = Vec::new();
                for (v, b) in incoming {
                    operands.push(Operand::IdRef(*self.val_cache.get(v).unwrap()));
                    operands.push(Operand::IdRef(*self.block_cache.get(b).unwrap()));
                }
                let spv_inst = Instruction::new(
                    spirv::Op::Phi,
                    Some(spirv_ty),
                    Some(id),
                    operands,
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::Cast { from_type, to_type, value } => {
                let val_spv = *self.val_cache.get(value).unwrap();
                let spv_op = conversion_op(from_type, to_type);
                let spv_inst = Instruction::new(
                    spv_op,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::IdRef(val_spv)],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::CompositeConstruct(constituents) => {
                let operands: Vec<Operand> = constituents
                    .iter()
                    .map(|c| Operand::IdRef(*self.val_cache.get(c).unwrap()))
                    .collect();
                let spv_inst = Instruction::new(
                    spirv::Op::CompositeConstruct,
                    Some(spirv_ty),
                    Some(id),
                    operands,
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::CompositeExtract { composite, index } => {
                let comp_spv = *self.val_cache.get(composite).unwrap();
                let spv_inst = Instruction::new(
                    spirv::Op::CompositeExtract,
                    Some(spirv_ty),
                    Some(id),
                    vec![Operand::IdRef(comp_spv), Operand::LiteralBit32(*index)],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::CompositeInsert {
                composite,
                value,
                index,
            } => {
                let comp_spv = *self.val_cache.get(composite).unwrap();
                let val_spv = *self.val_cache.get(value).unwrap();
                let spv_inst = Instruction::new(
                    spirv::Op::CompositeInsert,
                    Some(spirv_ty),
                    Some(id),
                    vec![
                        Operand::IdRef(val_spv),
                        Operand::IdRef(comp_spv),
                        Operand::LiteralBit32(*index),
                    ],
                );
                self.insert_in_block(spv_inst);
                self.val_cache.insert(inst.id, id);
            }
            Op::SelectionMerge(merge) => {
                let merge_spv = *self.block_cache.get(merge).unwrap();
                self.builder.selection_merge(merge_spv, spirv::SelectionControl::empty()).unwrap();
            }
            Op::LoopMerge { merge, continue_block } => {
                let merge_spv = *self.block_cache.get(merge).unwrap();
                let continue_spv = *self.block_cache.get(continue_block).unwrap();
                self.builder.loop_merge(merge_spv, continue_spv, spirv::LoopControl::empty(), []).unwrap();
            }
            Op::EnumConstruct { variant_index, payload } => {
                let enum_name = match &inst.ty {
                    Type::Enum(name) => name.clone(),
                    _ => panic!("EnumConstruct on non-enum type"),
                };
                let variants = self.enum_registry.get(&enum_name).cloned().unwrap_or_default();
                let has_payload = variants.iter().any(|(_, t)| *t != Type::Void);

                if has_payload {
                    let payload_ty = variants.iter().find_map(|(_, t)| if *t != Type::Void { Some(t.clone()) } else { None }).unwrap_or(Type::Void);
                    let payload_spv_ty = self.get_type(&payload_ty);
                    let disc_global = self.gen_spirv_constant(&Type::Scalar(ScalarType::U32), &Literal::Uint(*variant_index as u64));
                    let payload_val_spv = match payload {
                        Some(p) => *self.val_cache.get(p).unwrap(),
                        None => self.constant_zero(payload_spv_ty),
                    };

                    let spv_inst = Instruction::new(
                        spirv::Op::CompositeConstruct,
                        Some(spirv_ty),
                        Some(id),
                        vec![Operand::IdRef(disc_global), Operand::IdRef(payload_val_spv)],
                    );
                    self.insert_in_block(spv_inst);
                    self.val_cache.insert(inst.id, id);
                } else {
                    let const_id = self.gen_spirv_constant(&inst.ty, &Literal::Uint(*variant_index as u64));
                    self.val_cache.insert(inst.id, const_id);
                }
            }
        }
    }

    fn constant_zero(&mut self, spirv_ty: spirv::Word) -> spirv::Word {
        let id = self.builder.id();
        let inst = Instruction::new(
            spirv::Op::ConstantNull,
            Some(spirv_ty),
            Some(id),
            vec![],
        );
        self.builder.insert_types_global_values(InsertPoint::End, inst);
        return id;
    }

    fn gen_spirv_constant(&mut self, ty: &Type, lit: &Literal) -> spirv::Word {
        let spirv_ty = self.get_type(ty);
        return match lit {
            Literal::Bool(true) => self.builder.constant_true(spirv_ty),
            Literal::Bool(false) => self.builder.constant_false(spirv_ty),
            Literal::Int(v) => {
                let id = self.builder.id();
                let bits = constant_bits(ty, &Literal::Int(*v));
                let inst = Instruction::new(
                    spirv::Op::Constant,
                    Some(spirv_ty),
                    Some(id),
                    bits,
                );
                self.builder.insert_types_global_values(InsertPoint::End, inst);
                id
            }
            Literal::Uint(v) => {
                let id = self.builder.id();
                let bits = constant_bits(ty, &Literal::Uint(*v));
                let inst = Instruction::new(
                    spirv::Op::Constant,
                    Some(spirv_ty),
                    Some(id),
                    bits,
                );
                self.builder.insert_types_global_values(InsertPoint::End, inst);
                id
            }
            Literal::Float(v) => {
                let id = self.builder.id();
                let bits = constant_bits(ty, &Literal::Float(*v));
                let inst = Instruction::new(
                    spirv::Op::Constant,
                    Some(spirv_ty),
                    Some(id),
                    bits,
                );
                self.builder.insert_types_global_values(InsertPoint::End, inst);
                id
            }
        };
    }

    // --- terminators ---

    fn gen_terminator(&mut self, term: &Terminator) {
        match term {
            Terminator::Branch { target } => {
                let target_spv = *self.block_cache.get(target).unwrap();
                self.builder.branch(target_spv).unwrap();
            }
            Terminator::BranchCond {
                condition,
                true_target,
                false_target,
            } => {
                let cond_spv = *self.val_cache.get(condition).unwrap();
                let true_spv = *self.block_cache.get(true_target).unwrap();
                let false_spv = *self.block_cache.get(false_target).unwrap();
                self.builder.branch_conditional(cond_spv, true_spv, false_spv, []).unwrap();
            }
            Terminator::Return { value } => {
                if let Some(v) = value {
                    let val_spv = *self.val_cache.get(v).unwrap();
                    self.builder.ret_value(val_spv).unwrap();
                } else {
                    self.builder.ret().unwrap();
                }
            }
            Terminator::Unreachable => {
                self.builder.unreachable().unwrap();
            }
        }
    }

    fn insert_in_block(&mut self, inst: Instruction) {
        self.builder
            .insert_into_block(InsertPoint::End, inst)
            .unwrap();
    }
}

// --- mapping helpers ---

fn constant_bits(ty: &Type, lit: &Literal) -> Vec<Operand> {
    let is_64bit = matches!(ty, Type::Scalar(ScalarType::F64));
    if is_64bit {
        return match lit {
            Literal::Int(v) => vec![Operand::LiteralBit64(*v as u64)],
            Literal::Uint(v) => vec![Operand::LiteralBit64(*v as u64)],
            Literal::Float(v) => vec![Operand::LiteralBit64(v.to_bits())],
            _ => unreachable!(),
        };
    } else {
        return match lit {
            Literal::Int(v) => vec![Operand::LiteralBit32(*v as u32)],
            Literal::Uint(v) => vec![Operand::LiteralBit32(*v as u32)],
            Literal::Float(v) => vec![Operand::LiteralBit32((*v as f32).to_bits())],
            _ => unreachable!(),
        };
    }
}

fn spirv_storage_class(sc: StorageClass) -> spirv::StorageClass {
    return match sc {
        StorageClass::Function => spirv::StorageClass::Function,
        StorageClass::Descriptor => spirv::StorageClass::Uniform,
        StorageClass::PushConstant => spirv::StorageClass::PushConstant,
    };
}

fn binary_to_spirv(result_ty: &Type, input_ty: &Type, op: &BinaryOp) -> spirv::Op {
    let is_float = matches!(
        input_ty,
        Type::Scalar(ScalarType::F32 | ScalarType::F64)
            | Type::Vector(ScalarType::F32 | ScalarType::F64, _)
    );
    let is_signed = matches!(
        input_ty,
        Type::Scalar(ScalarType::I32) | Type::Vector(ScalarType::I32, _)
    );
    let is_bool = matches!(
        result_ty,
        Type::Scalar(ScalarType::Bool) | Type::Vector(ScalarType::Bool, _)
    );

    return match (op, is_float, is_signed, is_bool) {
        (BinaryOp::Add, true, _, _) => spirv::Op::FAdd,
        (BinaryOp::Add, false, _, _) => spirv::Op::IAdd,
        (BinaryOp::Subtract, true, _, _) => spirv::Op::FSub,
        (BinaryOp::Subtract, false, _, _) => spirv::Op::ISub,
        (BinaryOp::Multiply, true, _, _) => spirv::Op::FMul,
        (BinaryOp::Multiply, false, _, _) => spirv::Op::IMul,
        (BinaryOp::Divide, true, _, _) => spirv::Op::FDiv,
        (BinaryOp::Divide, false, true, _) => spirv::Op::SDiv,
        (BinaryOp::Divide, false, false, _) => spirv::Op::UDiv,
        (BinaryOp::Remainder, true, _, _) => spirv::Op::FRem,
        (BinaryOp::Remainder, false, true, _) => spirv::Op::SRem,
        (BinaryOp::Remainder, false, false, _) => spirv::Op::UMod,
        (BinaryOp::IsEqual, true, _, _) => spirv::Op::FOrdEqual,
        (BinaryOp::IsEqual, false, _, _) => spirv::Op::IEqual,
        (BinaryOp::IsNotEqual, true, _, _) => spirv::Op::FOrdNotEqual,
        (BinaryOp::IsNotEqual, false, _, _) => spirv::Op::INotEqual,
        (BinaryOp::LessThan, true, _, _) => spirv::Op::FOrdLessThan,
        (BinaryOp::LessThan, false, true, _) => spirv::Op::SLessThan,
        (BinaryOp::LessThan, false, false, _) => spirv::Op::ULessThan,
        (BinaryOp::LessEqual, true, _, _) => spirv::Op::FOrdLessThanEqual,
        (BinaryOp::LessEqual, false, true, _) => spirv::Op::SLessThanEqual,
        (BinaryOp::LessEqual, false, false, _) => spirv::Op::ULessThanEqual,
        (BinaryOp::GreaterThan, true, _, _) => spirv::Op::FOrdGreaterThan,
        (BinaryOp::GreaterThan, false, true, _) => spirv::Op::SGreaterThan,
        (BinaryOp::GreaterThan, false, false, _) => spirv::Op::UGreaterThan,
        (BinaryOp::GreaterEqual, true, _, _) => spirv::Op::FOrdGreaterThanEqual,
        (BinaryOp::GreaterEqual, false, true, _) => spirv::Op::SGreaterThanEqual,
        (BinaryOp::GreaterEqual, false, false, _) => spirv::Op::UGreaterThanEqual,
        (BinaryOp::And, _, _, true) => spirv::Op::LogicalAnd,
        (BinaryOp::And, _, _, false) => spirv::Op::BitwiseAnd,
        (BinaryOp::Or, _, _, true) => spirv::Op::LogicalOr,
        (BinaryOp::Or, _, _, false) => spirv::Op::BitwiseOr,
        (BinaryOp::BitAnd, _, _, _) => spirv::Op::BitwiseAnd,
        (BinaryOp::BitOr, _, _, _) => spirv::Op::BitwiseOr,
        (BinaryOp::BitXor, _, _, _) => spirv::Op::BitwiseXor,
        (BinaryOp::BitShiftL, _, _, _) => spirv::Op::ShiftLeftLogical,
        (BinaryOp::BitShiftR, _, true, _) => spirv::Op::ShiftRightArithmetic,
        (BinaryOp::BitShiftR, _, false, _) => spirv::Op::ShiftRightLogical,
    };
}

fn unary_to_spirv(op: &UnaryOp, ty: &Type) -> spirv::Op {
    let is_float = matches!(ty, Type::Scalar(ScalarType::F32 | ScalarType::F64) | Type::Vector(ScalarType::F32 | ScalarType::F64, _));
    return match (op, is_float) {
        (UnaryOp::Negate, true) => spirv::Op::FNegate,
        (UnaryOp::Negate, false) => spirv::Op::SNegate,
        (UnaryOp::Not, _) => spirv::Op::LogicalNot,
    };
}

fn conversion_op(from_type: &Type, to_type: &Type) -> spirv::Op {
    let from_float = matches!(from_type, Type::Scalar(ScalarType::F32 | ScalarType::F64));
    let to_float = matches!(to_type, Type::Scalar(ScalarType::F32 | ScalarType::F64));
    let from_signed = matches!(from_type, Type::Scalar(ScalarType::I32));
    let to_signed = matches!(to_type, Type::Scalar(ScalarType::I32));

    return match (from_float, to_float, from_signed, to_signed) {
        (true, false, _, true) => spirv::Op::ConvertFToS,
        (true, false, _, false) => spirv::Op::ConvertFToU,
        (false, true, true, _) => spirv::Op::ConvertSToF,
        (false, true, false, _) => spirv::Op::ConvertUToF,
        (false, false, true, false) => spirv::Op::SConvert,
        (false, false, false, true) => spirv::Op::UConvert,
        (false, false, _, _) => spirv::Op::Bitcast,
        _ => spirv::Op::Bitcast,
    };
}

fn stage_to_execution_model(stage: ShaderStage) -> spirv::ExecutionModel {
    return match stage {
        ShaderStage::Vertex => spirv::ExecutionModel::Vertex,
        ShaderStage::Fragment => spirv::ExecutionModel::Fragment,
        ShaderStage::Compute { .. } => spirv::ExecutionModel::GLCompute,
        ShaderStage::None => panic!("stage_to_execution_model: None has no ExecutionModel"),
    };
}

fn builtin_to_spirv(b: BuiltInVar) -> spirv::BuiltIn {
    return match b {
        BuiltInVar::VertexId => spirv::BuiltIn::VertexIndex,
        BuiltInVar::Position => spirv::BuiltIn::Position,
        BuiltInVar::FragCoord => spirv::BuiltIn::FragCoord,
    };
}
