use std::collections::HashMap;

use super::*;
use crate::resolver::{EnumRegistry, StructRegistry, *};

pub struct TypeChecker {
    resolved_tree: ResolvedSyntaxTree,
    variables: HashMap<VariableId, Type>,
    functions: HashMap<FunctionId, FunctionSignature>,
    errors: Vec<TypeError>,
    struct_registry: StructRegistry,
    enum_registry: EnumRegistry,
    loop_depth: u32,
}

#[derive(Debug, Clone)]
pub struct FunctionSignature {
    params: Vec<Type>,
    return_ty: Type,
}

#[derive(Debug)]
pub enum TypeError {
    MismatchedTypes,
    WrongFunctionArgs,
    WrongFunctionReturnType,
    UndefinedVariable,
    UndefinedFunction,
}

impl TypeChecker {
    pub fn new(resolved_tree: ResolvedSyntaxTree) -> TypeChecker {
        let struct_registry = resolved_tree.structs.clone();
        let enum_registry = resolved_tree.enums.clone();
        return TypeChecker {
            resolved_tree,
            variables: HashMap::new(),
            functions: HashMap::new(),
            errors: Vec::new(),
            struct_registry,
            enum_registry,
            loop_depth: 0,
        };
    }

    pub fn type_check(&mut self) -> (TypedSyntaxTree, Vec<TypeError>) {
        let descs = std::mem::take(&mut self.resolved_tree.descriptors);
        for var in &descs {
            self.check_descriptor(var);
        }
        self.resolved_tree.descriptors = descs;

        let globals = std::mem::take(&mut self.resolved_tree.globals);
        let mut typed_globals = vec![];
        for g in &globals {
            let typed_init = g.init.as_ref().map(|i| self.infer_expr_type(i));

            let ty = match (&g.var_type, &typed_init) {
                (Some(declared), Some(init)) => {
                    if init.ty != *declared {
                        self.errors.push(TypeError::MismatchedTypes);
                    }
                    declared.clone()
                }
                (Some(declared), None) => declared.clone(),
                (None, Some(init)) => init.ty.clone(),
                (None, None) => {
                    self.errors.push(TypeError::MismatchedTypes);
                    Type::Void
                }
            };

            self.variables.insert(g.id, ty.clone());

            typed_globals.push(TypedLocalVariable {
                mutable: false,
                id: g.id,
                debug_name: g.debug_name.clone(),
                var_type: ty,
                init: typed_init,
            });
        }
        self.resolved_tree.globals = globals;

        let mut typed_funcs = vec![];

        let funcs = std::mem::take(&mut self.resolved_tree.functions);
        for func in &funcs {
            let f = self.check_function(func);
            typed_funcs.push(f);
        }
        self.resolved_tree.functions = funcs;

        return (
            TypedSyntaxTree {
                functions: typed_funcs,
                descriptors: self.resolved_tree.descriptors.clone(),
                globals: typed_globals,
                structs: self.struct_registry.clone(),
                enums: self.enum_registry.clone(),
            },
            std::mem::take(&mut self.errors),
        );
    }
}

fn is_lvalue(expr: &ResolvedExpression) -> bool {
    matches!(expr, ResolvedExpression::Variable(_) | ResolvedExpression::Member { .. } | ResolvedExpression::Index { .. })
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty, Type::Scalar(s) if !matches!(s, ScalarType::Bool))
        || matches!(ty, Type::Vector(_, _))
}

// todo: add rules for types of 2 expressions
impl TypeChecker {
    #[inline]
    fn check_descriptor(&mut self, desc: &ResolvedDescriptor) {
        self.variables.insert(desc.id, desc.ty.clone());
    }

    fn check_function(&mut self, func: &ResolvedFunction) -> TypedFunction {
        let (typed_params, types) = func
            .params
            .iter()
            .map(|parms| {
                self.variables.insert(parms.id, parms.var_type.clone());

                (
                    TypedParameter {
                        id: parms.id,
                        debug_name: parms.debug_name.clone(),
                        var_type: parms.var_type.clone(),
                    },
                    parms.var_type.clone(),
                )
            })
            .collect();

        let typed_scope = self.check_scope(&func.body);

        if typed_scope.return_type != func.return_type {
            self.errors.push(TypeError::WrongFunctionReturnType);
        }

        self.functions.insert(
            func.id,
            FunctionSignature {
                params: types,
                return_ty: func.return_type.clone(),
            },
        );

        return TypedFunction {
            id: func.id,
            debug_name: func.debug_name.clone(),
            params: typed_params,
            return_type: func.return_type.clone(),
            body: typed_scope,
            stage: func.stage,
        };
    }

    fn check_scope(&mut self, scope: &ResolvedScope) -> TypedScope {
        let sts: Vec<TypedStatement> = scope.statements.iter().map(|st| self.check_statement(st)).collect();

        let ty = self.infer_scope_return_type(&sts);

        return TypedScope {
            statements: sts,
            return_type: ty,
        };
    }

    fn infer_scope_return_type(&self, statements: &[TypedStatement]) -> Type {
        if let Some(last) = statements.last() {
            match last {
                TypedStatement::Return(Some(expr)) => expr.ty.clone(),
                TypedStatement::Return(None) => Type::Void,
                TypedStatement::If { scopes, .. } => {
                    let return_types: Vec<&Type> = scopes
                        .iter()
                        .filter_map(|scope| match scope.return_type {
                            Type::Void => None,
                            ref t => Some(t),
                        })
                        .collect();
                    if return_types.len() == scopes.len() && !return_types.is_empty() {
                        return_types[0].clone()
                    } else {
                        Type::Void
                    }
                }
                TypedStatement::Loop { scope } => scope.return_type.clone(),
                TypedStatement::While { scope, .. } => scope.return_type.clone(),
                _ => Type::Void,
            }
        } else {
            Type::Void
        }
    }

    fn check_statement(&mut self, statement: &ResolvedStatement) -> TypedStatement {
        let statement = match statement {
            ResolvedStatement::Declaration(var) => {
                let typed_init = var.init.as_ref().map(|i| self.infer_expr_type(i));

                let ty = match (&var.var_type, &typed_init) {
                    (Some(declared), Some(init)) => {
                        if init.ty != *declared {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                        declared.clone()
                    }
                    (Some(declared), None) => declared.clone(),
                    (None, Some(init)) => init.ty.clone(),
                    (None, None) => {
                        // this is technically not an error. a stronger inference code can fix ts
                        self.errors.push(TypeError::MismatchedTypes);
                        Type::Void
                    }
                };

                if let (Some(typed), Type::Array(_, expected_len)) = (&typed_init, &ty) {
                    if let ExpressionKind::ArrayLiteral { values } = &typed.kind {
                        if values.len() as u32 != *expected_len {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                }

                self.variables.insert(var.id, ty.clone());

                TypedStatement::Declaration(TypedLocalVariable {
                    mutable: var.mutable,
                    id: var.id,
                    debug_name: var.debug_name.clone(),
                    var_type: ty,
                    init: typed_init,
                })
            }
            ResolvedStatement::Assign {
                target,
                value,
            } => {
                if !is_lvalue(target) {
                    self.errors.push(TypeError::MismatchedTypes);
                }

                let target = self.infer_expr_type(target);
                let value = self.infer_expr_type(value);

                if target.ty != value.ty {
                    self.errors.push(TypeError::MismatchedTypes);
                }

                TypedStatement::Assign {
                    target,
                    value,
                }
            }
            ResolvedStatement::If {
                scopes,
                conditions,
            } => {
                let cons: Vec<_> = conditions.iter().map(|expr| self.infer_expr_type(expr)).collect();
                let scopes: Vec<_> = scopes.iter().map(|scope| self.check_scope(scope)).collect();

                for con in &cons {
                    if con.ty != Type::Scalar(ScalarType::Bool) {
                        self.errors.push(TypeError::MismatchedTypes);
                    }
                }

                TypedStatement::If {
                    scopes,
                    conditions: cons,
                }
            }
            ResolvedStatement::Loop { scope } => {
                self.loop_depth += 1;
                let scope = self.check_scope(scope);
                self.loop_depth -= 1;

                TypedStatement::Loop { scope }
            }
            ResolvedStatement::While {
                condition,
                scope,
            } => {
                let condition = self.infer_expr_type(condition);

                if condition.ty != Type::Scalar(ScalarType::Bool) {
                    self.errors.push(TypeError::MismatchedTypes);
                }

                self.loop_depth += 1;
                let scope = self.check_scope(scope);
                self.loop_depth -= 1;

                TypedStatement::While {
                    condition,
                    scope,
                }
            }
            ResolvedStatement::FunctionCall(f) => {
                let expr = self.infer_expr_type(f);

                TypedStatement::FunctionCall(expr)
            }
            ResolvedStatement::Return(r) => {
                let expr = r.as_ref().map(|expr| self.infer_expr_type(&expr));

                TypedStatement::Return(expr)
            }
            ResolvedStatement::Break => {
                if self.loop_depth == 0 {
                    self.errors.push(TypeError::MismatchedTypes);
                }
                TypedStatement::Break
            }
            ResolvedStatement::Continue => {
                if self.loop_depth == 0 {
                    self.errors.push(TypeError::MismatchedTypes);
                }
                TypedStatement::Continue
            }
        };

        return statement;
    }

    fn infer_expr_type(&mut self, expr: &ResolvedExpression) -> TypedExpression {
        return match expr {
            ResolvedExpression::Literal(l) => {
                let ty = match *l {
                    Literal::Bool(_) => Type::Scalar(ScalarType::Bool),
                    Literal::Float(_) => Type::Scalar(ScalarType::F32),
                    Literal::Int(_) => Type::Scalar(ScalarType::I32),
                    Literal::Uint(_) => Type::Scalar(ScalarType::U32),
                };

                TypedExpression {
                    ty: ty,
                    kind: ExpressionKind::Literal(*l),
                }
            }
            ResolvedExpression::Variable(id) => {
                let ty = self.variables.get(id).cloned().unwrap_or_else(|| Type::Void);

                TypedExpression {
                    ty: ty,
                    kind: ExpressionKind::Variable(*id),
                }
            }
            ResolvedExpression::Unary { op, expr } => {
                let expr = self.infer_expr_type(expr);

                match op {
                    UnaryOp::Not => {
                        if expr.ty != Type::Scalar(ScalarType::Bool) {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                    UnaryOp::Negate => {
                        if !is_numeric(&expr.ty) {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                }

                TypedExpression {
                    ty: expr.ty.clone(),
                    kind: ExpressionKind::Unary {
                        op: *op,
                        expr: Box::new(expr),
                    },
                }
            }
            ResolvedExpression::Binary {
                op,
                left,
                right,
            } => {
                let lhs = self.infer_expr_type(left);
                let rhs = self.infer_expr_type(right);

                let ty = match op {
                    BinaryOp::IsEqual | BinaryOp::IsNotEqual
                    | BinaryOp::LessThan | BinaryOp::LessEqual
                    | BinaryOp::GreaterThan | BinaryOp::GreaterEqual
                    | BinaryOp::And | BinaryOp::Or => Type::Scalar(ScalarType::Bool),
                    _ => lhs.ty.clone(),
                };

                match op {
                    BinaryOp::IsEqual | BinaryOp::IsNotEqual
                    | BinaryOp::LessThan | BinaryOp::LessEqual
                    | BinaryOp::GreaterThan | BinaryOp::GreaterEqual
                    | BinaryOp::And | BinaryOp::Or => {}
                    _ => {
                        if !is_numeric(&lhs.ty) || !is_numeric(&rhs.ty) {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                }

                if lhs.ty != rhs.ty {
                    self.errors.push(TypeError::MismatchedTypes);
                }

                TypedExpression {
                    ty: ty,
                    kind: ExpressionKind::Binary {
                        op: *op,
                        left: Box::new(lhs),
                        right: Box::new(rhs),
                    },
                }
            }

            ResolvedExpression::Call {
                function,
                args,
            } => {
                let sign = match self.functions.get(function) {
                    Some(s) => s.clone(),
                    None => {
                        self.errors.push(TypeError::UndefinedFunction);
                        return TypedExpression {
                            ty: Type::Void,
                            kind: ExpressionKind::Call {
                                function: *function,
                                args: vec![],
                            },
                        };
                    }
                };

                if args.len() != sign.params.len() {
                    self.errors.push(TypeError::WrongFunctionArgs);
                    return TypedExpression {
                        ty: sign.return_ty.clone(),
                        kind: ExpressionKind::Call {
                            function: *function,
                            args: vec![],
                        },
                    };
                }

                let mut typed_args = vec![];

                for i in 0..args.len() {
                    let typed_arg = self.infer_expr_type(&args[i]);

                    if typed_arg.ty != sign.params[i] {
                        self.errors.push(TypeError::WrongFunctionArgs);
                    }
                    typed_args.push(typed_arg);
                }

                TypedExpression {
                    ty: sign.return_ty.clone(),
                    kind: ExpressionKind::Call {
                        function: *function,
                        args: typed_args,
                    },
                }
            }

            ResolvedExpression::StructLiteral {
                struct_name,
                fields,
            } => {
                let struct_fields = self.struct_registry.get(struct_name).cloned().unwrap_or_default();
                let resolved_fields: Vec<(String, TypedExpression)> = fields
                    .iter()
                    .map(|(fname, fexpr)| {
                        let typed = self.infer_expr_type(fexpr);
                        let expected = struct_fields.iter().find(|f| f.name == *fname);
                        if let Some(fi) = expected {
                            if typed.ty != fi.ty {
                                self.errors.push(TypeError::MismatchedTypes);
                            }
                        } else {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                        (fname.clone(), typed)
                    })
                    .collect();

                if fields.len() != struct_fields.len() {
                    self.errors.push(TypeError::MismatchedTypes);
                }

                TypedExpression {
                    ty: Type::Struct(struct_name.clone()),
                    kind: ExpressionKind::StructLiteral {
                        struct_name: struct_name.clone(),
                        fields: resolved_fields,
                    },
                }
            }
            ResolvedExpression::Member {
                base,
                field,
            } => {
                let base = self.infer_expr_type(base);
                let (field_type, field_index) = match &base.ty {
                    Type::Struct(sname) => match self.struct_registry.get(sname) {
                        Some(sfields) => sfields
                            .iter()
                            .position(|f| f.name == *field)
                            .map(|idx| (sfields[idx].ty.clone(), idx as u32))
                            .unwrap_or_else(|| {
                                self.errors.push(TypeError::MismatchedTypes);
                                (Type::Void, 0)
                            }),
                        None => {
                            self.errors.push(TypeError::MismatchedTypes);
                            (Type::Void, 0)
                        }
                    },
                    _ => {
                        self.errors.push(TypeError::MismatchedTypes);
                        (Type::Void, 0)
                    }
                };
                TypedExpression {
                    ty: field_type,
                    kind: ExpressionKind::Member {
                        base: Box::new(base),
                        field: field.clone(),
                        field_index,
                    },
                }
            }
            ResolvedExpression::Index {
                base,
                index,
            } => {
                let base = self.infer_expr_type(base);
                let index = self.infer_expr_type(index);

                TypedExpression {
                    ty: match &base.ty {
                        Type::Array(ty, _) => ty.as_ref().clone(),
                        Type::Vector(ty, _) => Type::Scalar(*ty),
                        _ => {
                            self.errors.push(TypeError::MismatchedTypes);
                            Type::Void
                        }
                    },
                    kind: ExpressionKind::Index {
                        base: Box::new(base),
                        index: Box::new(index),
                    },
                }
            }
            ResolvedExpression::Cast { ty, expr } => {
                let expr = self.infer_expr_type(expr);
                TypedExpression {
                    ty: ty.clone(),
                    kind: ExpressionKind::Cast {
                        ty: ty.clone(),
                        expr: Box::new(expr),
                    },
                }
            }
            ResolvedExpression::EnumLiteral {
                enum_name,
                variant,
                variant_index,
                payload,
            } => {
                let resolved_payload = payload.as_ref().map(|p| Box::new(self.infer_expr_type(p)));
                if let Some(ref p) = resolved_payload {
                    let variants = self.enum_registry.get(enum_name).cloned().unwrap_or_default();
                    if let Some((_, expected_ty)) = variants.iter().find(|(n, _)| n == variant) {
                        if p.ty != *expected_ty {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                }
                TypedExpression {
                    ty: Type::Enum(enum_name.clone()),
                    kind: ExpressionKind::EnumLiteral {
                        enum_name: enum_name.clone(),
                        variant: variant.clone(),
                        variant_index: *variant_index,
                        payload: resolved_payload,
                    },
                }
            }
            ResolvedExpression::ArrayLiteral(arr) => match arr {
                ResolvedArrayLiteral::Repeat {
                    value,
                    count,
                } => {
                    let typed_value = self.infer_expr_type(value);
                    let element_ty = typed_value.ty.clone();
                    let values = (0..*count).map(|_| typed_value.clone()).collect();
                    TypedExpression {
                        ty: Type::Array(Box::new(element_ty), *count),
                        kind: ExpressionKind::ArrayLiteral { values },
                    }
                }
                ResolvedArrayLiteral::Normal { values } => {
                    let typed: Vec<_> = values.iter().map(|v| self.infer_expr_type(v)).collect();
                    let element_ty = typed.first().map(|v| v.ty.clone()).unwrap_or(Type::Void);
                    for v in &typed {
                        if v.ty != element_ty {
                            self.errors.push(TypeError::MismatchedTypes);
                        }
                    }
                    TypedExpression {
                        ty: Type::Array(Box::new(element_ty), typed.len() as u32),
                        kind: ExpressionKind::ArrayLiteral {
                            values: typed,
                        },
                    }
                }
            },
        };
    }
}
