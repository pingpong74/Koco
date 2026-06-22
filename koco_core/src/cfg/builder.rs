use std::collections::HashMap;

use super::*;
use crate::resolver::*;
use crate::type_checking::*;

pub struct CfgBuilder {
    next_value_id: ValueId,
    next_block_id: BlockId,
    blocks: Vec<BasicBlock>,
    current_block: BlockId,
    var_to_ptr: HashMap<VariableId, ValueId>,
    loop_stack: Vec<LoopFrame>,
    typed_globals: Vec<TypedLocalVariable>,
}

struct LoopFrame {
    continue_block: BlockId,
    merge_block: BlockId,
}

impl CfgBuilder {
    pub fn new() -> Self {
        return Self {
            next_value_id: 0,
            next_block_id: 0,
            blocks: Vec::new(),
            current_block: 0,
            var_to_ptr: HashMap::new(),
            loop_stack: Vec::new(),
            typed_globals: Vec::new(),
        };
    }

    pub fn build(&mut self, tree: TypedSyntaxTree) -> CfgModule {
        self.typed_globals = tree.globals;

        let functions = tree.functions.iter().map(|f| self.build_function(f)).collect();

        let descriptors = tree.descriptors.iter().map(|d| self.build_descriptor(d)).collect();

        return CfgModule {
            functions,
            descriptors,
        };
    }

    fn build_descriptor(&mut self, desc: &ResolvedDescriptor) -> CfgDescriptor {
        let id = self.next_value_id();
        return CfgDescriptor {
            id,
            name: desc.debug_name.clone(),
            ty: desc.ty.clone(),
            storage: desc.storage,
            binding: desc.binding,
        };
    }

    fn build_function(&mut self, func: &TypedFunction) -> CfgFunction {
        self.var_to_ptr.clear();
        self.loop_stack.clear();
        self.blocks.clear();
        self.next_value_id = 0;
        self.next_block_id = 0;

        let entry = self.new_block_internal();
        self.current_block = entry;

        let params = func
            .params
            .iter()
            .map(|param| {
                let ptr_ty = Type::Pointer(Box::new(param.var_type.clone()), StorageClass::Function);
                let ptr = self.emit_internal(ptr_ty, Op::Variable(StorageClass::Function));
                self.var_to_ptr.insert(param.id, ptr);
                CfgParameter {
                    id: ptr,
                    name: param.debug_name.clone(),
                    ty: param.var_type.clone(),
                }
            })
            .collect();

        let globals: Vec<_> = self
            .typed_globals
            .iter()
            .map(|g| (g.id, g.var_type.clone(), g.init.clone()))
            .collect();

        for (gid, var_type, _) in &globals {
            let ptr_ty = Type::Pointer(Box::new(var_type.clone()), StorageClass::Function);
            let ptr = self.emit_internal(ptr_ty, Op::Variable(StorageClass::Function));
            self.var_to_ptr.insert(*gid, ptr);
        }

        // Collect and allocate local variable pointers
        let local_decls: Vec<(VariableId, Type)> = collect_declarations(&func.body);
        for (var_id, var_type) in &local_decls {
            let ptr_ty = Type::Pointer(Box::new(var_type.clone()), StorageClass::Function);
            let ptr = self.emit_internal(ptr_ty, Op::Variable(StorageClass::Function));
            self.var_to_ptr.insert(*var_id, ptr);
        }

        // Initialize globals with their initializers
        for (gid, _, init) in &globals {
            if let Some(init_expr) = init {
                let val = self.emit_rvalue(init_expr);
                let ptr = *self.var_to_ptr.get(gid).unwrap();
                self.emit_internal(Type::Void, Op::Store(ptr, val));
            }
        }

        // Emit the function body
        self.emit_scope(&func.body);

        // If the last block is void and has no terminator, add implicit return
        let last = &self.blocks[self.current_block as usize];
        if matches!(last.terminator, Terminator::Unreachable) && func.return_type == Type::Void {
            self.set_terminator(Terminator::Return {
                value: None,
            });
        }

        let signature = FunctionType {
            param_types: func.params.iter().map(|p| p.var_type.clone()).collect(),
            return_type: func.return_type.clone(),
        };

        let mut blocks = std::mem::take(&mut self.blocks);
        reorder_blocks(&mut blocks, entry);

        return CfgFunction {
            id: func.id,
            name: func.debug_name.clone(),
            signature,
            parameters: params,
            blocks,
            entry_block: entry,
            stage: func.stage,
        };
    }

    fn emit_scope(&mut self, scope: &TypedScope) {
        for stmt in &scope.statements {
            if !self.block_terminator_is_unreachable() {
                break;
            }
            self.emit_statement(stmt);
        }
    }

    fn emit_statement(&mut self, stmt: &TypedStatement) {
        match stmt {
            TypedStatement::Declaration(var) => {
                let ptr = if let Some(&ptr) = self.var_to_ptr.get(&var.id) {
                    ptr
                } else {
                    let ptr_ty = Type::Pointer(Box::new(var.var_type.clone()), StorageClass::Function);
                    let ptr = self.emit(ptr_ty, Op::Variable(StorageClass::Function));
                    self.var_to_ptr.insert(var.id, ptr);
                    ptr
                };
                if let Some(init) = &var.init {
                    let val = self.emit_rvalue(init);
                    self.emit(Type::Void, Op::Store(ptr, val));
                }
            }
            TypedStatement::Assign {
                target,
                value,
            } => {
                let val = self.emit_rvalue(value);
                let ptr = self.emit_lvalue(target);
                self.emit(Type::Void, Op::Store(ptr, val));
            }
            TypedStatement::If {
                scopes,
                conditions,
            } => {
                self.emit_if(scopes, conditions);
            }
            TypedStatement::Loop { scope } => {
                self.emit_loop(scope);
            }
            TypedStatement::While {
                condition,
                scope,
            } => {
                self.emit_while(condition, scope);
            }
            TypedStatement::FunctionCall(expr) => {
                self.emit_rvalue(expr);
            }
            TypedStatement::Return(ret) => {
                let val = ret.as_ref().map(|e| self.emit_rvalue(e));
                self.set_terminator(Terminator::Return { value: val });
            }
            TypedStatement::Break => {
                let frame = self.loop_stack.last().expect("break outside loop");
                self.set_terminator(Terminator::Branch {
                    target: frame.merge_block,
                });
            }
            TypedStatement::Continue => {
                let frame = self.loop_stack.last().expect("continue outside loop");
                self.set_terminator(Terminator::Branch {
                    target: frame.continue_block,
                });
            }
        }
    }

    fn emit_if(&mut self, scopes: &[TypedScope], conditions: &[TypedExpression]) {
        let merge = self.new_block();

        for (i, (scope, cond)) in scopes.iter().zip(conditions.iter()).enumerate() {
            let cond_val = self.emit_rvalue(cond);
            let true_block = self.new_block();
            let false_block = if i + 1 < scopes.len() || scopes.len() > conditions.len() {
                self.new_block()
            } else {
                merge
            };

            self.emit(Type::Void, Op::SelectionMerge(merge));
            self.set_terminator(Terminator::BranchCond {
                condition: cond_val,
                true_target: true_block,
                false_target: false_block,
            });

            self.current_block = true_block;
            self.emit_scope(scope);
            if self.block_terminator_is_unreachable() {
                self.set_terminator(Terminator::Branch {
                    target: merge,
                });
            }

            self.current_block = false_block;
        }

        // Emit trailing else scope (no corresponding condition)
        if scopes.len() > conditions.len() {
            let else_scope = &scopes[scopes.len() - 1];
            self.emit_scope(else_scope);
            if self.block_terminator_is_unreachable() {
                self.set_terminator(Terminator::Branch {
                    target: merge,
                });
            }
        }

        self.current_block = merge;
    }

    fn emit_loop(&mut self, scope: &TypedScope) {
        let header = self.new_block();
        let body = self.new_block();
        let continue_block = self.new_block();
        let merge = self.new_block();

        self.set_terminator(Terminator::Branch {
            target: header,
        });

        self.current_block = header;
        self.emit(
            Type::Void,
            Op::LoopMerge {
                merge,
                continue_block,
            },
        );
        self.set_terminator(Terminator::Branch {
            target: body,
        });

        self.current_block = body;
        self.loop_stack.push(LoopFrame {
            continue_block,
            merge_block: merge,
        });
        self.emit_scope(scope);
        self.loop_stack.pop();

        if self.block_terminator_is_unreachable() {
            self.set_terminator(Terminator::Branch {
                target: continue_block,
            });
        }

        self.current_block = continue_block;
        self.set_terminator(Terminator::Branch {
            target: header,
        });

        self.current_block = merge;
    }

    fn emit_while(&mut self, condition: &TypedExpression, scope: &TypedScope) {
        let header = self.new_block();
        let body = self.new_block();
        let continue_block = self.new_block();
        let merge = self.new_block();
        self.set_terminator(Terminator::Branch {
            target: header,
        });
        self.current_block = header;
        let cond_val = self.emit_rvalue(condition);
        self.emit(
            Type::Void,
            Op::LoopMerge {
                merge,
                continue_block,
            },
        );
        self.set_terminator(Terminator::BranchCond {
            condition: cond_val,
            true_target: body,
            false_target: merge,
        });
        self.current_block = body;
        self.loop_stack.push(LoopFrame {
            continue_block,
            merge_block: merge,
        });
        self.emit_scope(scope);
        self.loop_stack.pop();
        if self.block_terminator_is_unreachable() {
            self.set_terminator(Terminator::Branch {
                target: continue_block,
            });
        }
        self.current_block = continue_block;
        self.set_terminator(Terminator::Branch {
            target: header,
        });
        self.current_block = merge;
    }

    fn emit_rvalue(&mut self, expr: &TypedExpression) -> ValueId {
        return match &expr.kind {
            ExpressionKind::Literal(lit) => self.emit(expr.ty.clone(), Op::Constant(*lit)),
            ExpressionKind::Variable(id) => {
                let ptr = self.get_or_alloc_global_ptr(*id);
                self.emit(expr.ty.clone(), Op::Load(ptr))
            }
            ExpressionKind::Binary {
                op,
                left,
                right,
            } => {
                let l = self.emit_rvalue(left);
                let r = self.emit_rvalue(right);
                self.emit(expr.ty.clone(), Op::Binary(*op, l, r, left.ty.clone()))
            }
            ExpressionKind::Unary {
                op,
                expr: inner,
            } => {
                let v = self.emit_rvalue(inner);
                self.emit(expr.ty.clone(), Op::Unary(*op, v))
            }
            ExpressionKind::Call {
                function,
                args,
            } => {
                let args: Vec<_> = args.iter().map(|a| self.emit_rvalue(a)).collect();
                self.emit(expr.ty.clone(), Op::Call(*function, args))
            }
            ExpressionKind::StructLiteral {
                struct_name: _,
                fields,
            } => {
                let constituents: Vec<ValueId> = fields.iter().map(|(_, fexpr)| self.emit_rvalue(fexpr)).collect();
                self.emit(expr.ty.clone(), Op::CompositeConstruct(constituents))
            }
            ExpressionKind::Member {
                base,
                field_index,
                ..
            } => {
                let base = self.emit_rvalue(base);
                self.emit(
                    expr.ty.clone(),
                    Op::CompositeExtract {
                        composite: base,
                        index: *field_index,
                    },
                )
            }
            ExpressionKind::Index {
                base,
                index,
            } => {
                let base_ptr = self.emit_lvalue(base);
                let idx_val = self.emit_rvalue(index);
                let elem_ptr_ty = Type::Pointer(Box::new(expr.ty.clone()), StorageClass::Function);
                let elem_ptr = self.emit(
                    elem_ptr_ty,
                    Op::AccessChain {
                        base: base_ptr,
                        indices: vec![idx_val],
                    },
                );
                self.emit(expr.ty.clone(), Op::Load(elem_ptr))
            }
            ExpressionKind::Cast {
                ty,
                expr: inner,
            } => {
                let v = self.emit_rvalue(inner);
                self.emit(
                    expr.ty.clone(),
                    Op::Cast {
                        from_type: inner.ty.clone(),
                        to_type: ty.clone(),
                        value: v,
                    },
                )
            }
            ExpressionKind::EnumLiteral {
                variant_index,
                payload,
                ..
            } => {
                let payload_id = payload.as_ref().map(|p| self.emit_rvalue(p));
                self.emit(
                    expr.ty.clone(),
                    Op::EnumConstruct {
                        variant_index: *variant_index,
                        payload: payload_id,
                    },
                )
            }
            ExpressionKind::ArrayLiteral { values } => {
                let constituents: Vec<ValueId> = values.iter().map(|v| self.emit_rvalue(v)).collect();
                self.emit(expr.ty.clone(), Op::CompositeConstruct(constituents))
            }
        };
    }

    fn emit_lvalue(&mut self, expr: &TypedExpression) -> ValueId {
        return match &expr.kind {
            ExpressionKind::Variable(id) => self.get_or_alloc_global_ptr(*id),
            ExpressionKind::Member {
                base,
                field_index,
                ..
            } => {
                let base_ptr = self.emit_lvalue(base);
                let idx = self.emit(Type::Scalar(ScalarType::U32), Op::Constant(Literal::Uint(*field_index as u64)));
                let ptr_ty = Type::Pointer(Box::new(expr.ty.clone()), StorageClass::Function);
                self.emit(
                    ptr_ty,
                    Op::AccessChain {
                        base: base_ptr,
                        indices: vec![idx],
                    },
                )
            }
            ExpressionKind::Index {
                base,
                index,
            } => {
                let base_ptr = self.emit_lvalue(base);
                let idx = self.emit_rvalue(index);
                let ptr_ty = Type::Pointer(Box::new(expr.ty.clone()), StorageClass::Function);
                self.emit(
                    ptr_ty,
                    Op::AccessChain {
                        base: base_ptr,
                        indices: vec![idx],
                    },
                )
            }
            _ => panic!("emit_lvalue: not an lvalue"),
        };
    }

    fn get_or_alloc_global_ptr(&mut self, id: VariableId) -> ValueId {
        if let Some(&ptr) = self.var_to_ptr.get(&id) {
            return ptr;
        }
        let idx = match self.typed_globals.iter().position(|g| g.id == id) {
            Some(i) => i,
            None => panic!("variable {:?} not allocated", id),
        };
        let var_type = self.typed_globals[idx].var_type.clone();
        let init = self.typed_globals[idx].init.clone();
        let ptr_ty = Type::Pointer(Box::new(var_type.clone()), StorageClass::Function);
        let ptr = self.emit_internal(ptr_ty, Op::Variable(StorageClass::Function));
        self.var_to_ptr.insert(id, ptr);
        if let Some(init_expr) = &init {
            let val = self.emit_rvalue(init_expr);
            self.emit_internal(Type::Void, Op::Store(ptr, val));
        }
        return ptr;
    }

    fn new_block(&mut self) -> BlockId {
        let id = self.next_block_id;
        self.next_block_id += 1;
        self.blocks.push(BasicBlock {
            id,
            instructions: Vec::new(),
            terminator: Terminator::Unreachable,
        });
        return id;
    }

    fn new_block_internal(&mut self) -> BlockId {
        return self.new_block();
    }

    fn emit(&mut self, ty: Type, op: Op) -> ValueId {
        let id = self.next_value_id;
        self.next_value_id += 1;
        self.blocks[self.current_block as usize]
            .instructions
            .push(Instruction { id, ty, op });
        return id;
    }

    fn emit_internal(&mut self, ty: Type, op: Op) -> ValueId {
        return self.emit(ty, op);
    }

    fn set_terminator(&mut self, terminator: Terminator) {
        self.blocks[self.current_block as usize].terminator = terminator;
    }

    fn next_value_id(&mut self) -> ValueId {
        let id = self.next_value_id;
        self.next_value_id += 1;
        return id;
    }

    fn block_terminator_is_unreachable(&self) -> bool {
        return matches!(self.blocks[self.current_block as usize].terminator, Terminator::Unreachable);
    }
}

// used to get all the variable declarations
fn collect_declarations(scope: &TypedScope) -> Vec<(VariableId, Type)> {
    let mut result = Vec::new();
    for stmt in &scope.statements {
        match stmt {
            TypedStatement::Declaration(var) => {
                result.push((var.id, var.var_type.clone()));
            }
            TypedStatement::If { scopes, .. } => {
                for s in scopes {
                    result.extend(collect_declarations(s));
                }
            }
            TypedStatement::Loop { scope: s } => {
                result.extend(collect_declarations(s));
            }
            TypedStatement::While {
                scope: s, ..
            } => {
                result.extend(collect_declarations(s));
            }
            _ => {}
        }
    }
    return result;
}

fn reorder_blocks(blocks: &mut Vec<BasicBlock>, entry: BlockId) {
    let n = blocks.len();
    if n == 0 {
        return;
    }
    let mut visited = vec![false; n];
    let mut order = Vec::with_capacity(n);

    fn dfs(blocks: &[BasicBlock], id: BlockId, visited: &mut [bool], order: &mut Vec<BlockId>) {
        if visited[id as usize] {
            return;
        }
        visited[id as usize] = true;
        let term = &blocks[id as usize].terminator;
        match term {
            Terminator::Branch { target } => dfs(blocks, *target, visited, order),
            Terminator::BranchCond {
                true_target,
                false_target,
                ..
            } => {
                dfs(blocks, *true_target, visited, order);
                dfs(blocks, *false_target, visited, order);
            }
            Terminator::Return { .. } | Terminator::Unreachable => {}
        }
        order.push(id);
    }

    dfs(blocks, entry, &mut visited, &mut order);
    order.reverse();

    let unreachable: Vec<BasicBlock> = (0..n).filter(|&i| !visited[i]).map(|i| blocks[i].clone()).collect();

    let mut reordered: Vec<BasicBlock> = order.iter().map(|&id| blocks[id as usize].clone()).collect();
    reordered.extend(unreachable);
    *blocks = reordered;
}
