use super::*;
use crate::SyntaxTree;
use std::collections::HashMap;

pub struct Resolver {
    syntax_tree: SyntaxTree,

    function_names: HashMap<String, FunctionId>,
    scopes: Vec<HashMap<String, (VariableId, bool)>>,
    global_names: HashMap<String, VariableId>,
    struct_registry: StructRegistry,
    enum_registry: EnumRegistry,

    next_local_id: u32,
    errors: Vec<ResolveError>,
}

#[derive(Debug, Clone)]
pub enum ResolveError {
    Undefined(String),
    AlreadyDeclared(String),
    AssignToImmutable(String),
    AssignToBuiltin(String),
    InvalidAssignment,
    NotSupported(String),
}

impl Resolver {
    pub fn new(tree: SyntaxTree) -> Resolver {
        return Resolver {
            syntax_tree: tree,
            function_names: HashMap::new(),
            scopes: Vec::new(),
            global_names: HashMap::new(),
            struct_registry: HashMap::new(),
            enum_registry: HashMap::new(),
            next_local_id: 0,
            errors: Vec::new(),
        };
    }

    pub fn resolve(&mut self) -> (ResolvedSyntaxTree, Vec<ResolveError>) {
        let mut resolved_desc = Vec::new();

        for (i, f) in self.syntax_tree.functions.iter().enumerate() {
            self.function_names.insert(
                f.name.clone(),
                FunctionId {
                    id: i as u32,
                },
            );
        }

        // Register enum definitions
        for e in &self.syntax_tree.enums {
            let variants: EnumVariants = e
                .kinds
                .iter()
                .map(|(name, ty)| (name.clone(), self.resolve_type(ty.clone())))
                .collect();
            self.enum_registry.insert(e.name.clone(), variants);
        }

        // Register struct definitions
        for s in &self.syntax_tree.structs {
            let fields: StructFields = s
                .fields
                .iter()
                .map(|(name, ty, attrs)| FieldInfo {
                    name: name.clone(),
                    ty: self.resolve_type(ty.clone()),
                    location: attrs.location,
                    builtin: attrs.builtin.as_ref().and_then(|b| Self::lookup_builtin(b)),
                })
                .collect();
            self.struct_registry.insert(s.name.clone(), fields);
        }

        // First pass: register names for descriptors and const globals
        for desc in &self.syntax_tree.descriptors {
            let id = VariableId::Global(self.global_names.len() as u32);
            self.global_names.insert(desc.name.clone(), id);

            resolved_desc.push(ResolvedDescriptor {
                id,
                debug_name: desc.name.clone(),
                ty: self.resolve_type(desc.ty.clone()),
                storage: desc.storage,
                binding: desc.binding,
            });
        }

        for stmt in &self.syntax_tree.global_variables {
            if let Statement::Declaration(var) = stmt {
                let id = VariableId::Global(self.global_names.len() as u32);
                self.global_names.insert(var.name.clone(), id);
            }
        }

        // Second pass: resolve init expressions for const globals
        let mut resolved_globals = vec![];
        let global_stmts = std::mem::take(&mut self.syntax_tree.global_variables);
        for stmt in &global_stmts {
            if let Statement::Declaration(var) = stmt {
                let id = *self.global_names.get(&var.name).unwrap();
                let debug_name = var.name.clone();
                let var_type = var.var_type.clone().map(|t| self.resolve_type(t));
                let init = var.init.as_ref().map(|e| self.resolve_expression(e));
                resolved_globals.push(ResolvedLocalVariable {
                    mutable: false,
                    id,
                    debug_name,
                    var_type,
                    init,
                });
            }
        }
        self.syntax_tree.global_variables = global_stmts;

        let funcs = std::mem::take(&mut self.syntax_tree.functions);
        let mut functions = vec![];

        for f in &funcs {
            functions.push(self.resolve_function(f));
        }

        self.syntax_tree.functions = funcs;

        return (
            ResolvedSyntaxTree {
                functions,
                descriptors: resolved_desc,
                globals: resolved_globals,
                structs: self.struct_registry.clone(),
                enums: self.enum_registry.clone(),
            },
            std::mem::take(&mut self.errors),
        );
    }

    fn look_up_var(&self, name: &str) -> Result<(VariableId, bool), ResolveError> {
        for scope in self.scopes.iter().rev() {
            if let Some((id, m)) = scope.get(name) {
                return Ok((*id, *m));
            }
        }

        if let Some(id) = self.global_names.get(name) {
            return Ok((*id, false));
        }

        if let Some(b) = Self::lookup_builtin(name) {
            return Ok((VariableId::Builtin(b), false));
        }

        return Err(ResolveError::Undefined(name.to_string()));
    }

    fn lookup_builtin(name: &str) -> Option<BuiltInVar> {
        return match name {
            "_vertex_id" => Some(BuiltInVar::VertexId),
            "position" => Some(BuiltInVar::Position),
            "frag_coord" => Some(BuiltInVar::FragCoord),
            _ => None,
        };
    }

    fn declare_local(&mut self, name: String, mutable: bool) -> Result<VariableId, ResolveError> {
        let scope = self.scopes.last_mut().unwrap();
        if scope.contains_key(&name) {
            return Err(ResolveError::AlreadyDeclared(name));
        }

        let id = {
            let id = VariableId::Local(self.next_local_id);
            self.next_local_id += 1;

            id
        };

        scope.insert(name, (id, mutable));
        return Ok(id);
    }

    fn dummy_id() -> VariableId {
        VariableId::Local(u32::MAX)
    }

    fn base_variable(expr: &Expression) -> Option<String> {
        match expr {
            Expression::Variable(name) => Some(name.clone()),
            Expression::Member {
                object,
                field: _,
            } => Self::base_variable(object),
            Expression::Index {
                object,
                index: _,
            } => Self::base_variable(object),
            _ => None,
        }
    }

    fn resolve_type(&self, pt: ParserType) -> Type {
        match pt {
            ParserType::Void => Type::Void,
            ParserType::Single(name) => {
                if self.enum_registry.contains_key(&name) {
                    Type::Enum(name)
                } else {
                    Type::from_name(&name)
                }
            }
            ParserType::Array(inner, len) => Type::Array(Box::new(self.resolve_type(*inner)), len),
        }
    }
}

impl Resolver {
    fn resolve_function(&mut self, func: &Function) -> ResolvedFunction {
        self.scopes.push(HashMap::new());

        let mut resolved_params = vec![];

        for param in &func.params {
            let id = match self.declare_local(param.name.clone(), false) {
                Ok(id) => id,
                Err(e) => {
                    self.errors.push(e);
                    Self::dummy_id()
                }
            };
            resolved_params.push(ResolvedParameter {
                id: id,
                debug_name: param.name.clone(),
                var_type: self.resolve_type(param.var_type.clone()),
            });
        }

        let body = self.resolve_scope(&func.body);
        self.scopes.pop();

        return ResolvedFunction {
            debug_name: func.name.clone(),
            id: *self.function_names.get(&func.name).unwrap(),
            params: resolved_params,
            return_type: self.resolve_type(func.return_type.clone()),
            body,
            stage: func.stage.clone(),
        };
    }

    fn resolve_scope(&mut self, scope: &Scope) -> ResolvedScope {
        self.scopes.push(HashMap::new());
        let mut statements = vec![];

        for stmt in &scope.statements {
            statements.push(self.resolve_statement(stmt));
        }

        self.scopes.pop();

        return ResolvedScope { statements };
    }

    fn resolve_statement(&mut self, stmt: &Statement) -> ResolvedStatement {
        return match stmt {
            Statement::Declaration(local) => {
                let init = local.init.as_ref().map(|e| self.resolve_expression(e));

                let id = match self.declare_local(local.name.clone(), local.mutable) {
                    Ok(id) => id,
                    Err(e) => {
                        self.errors.push(e);
                        Self::dummy_id()
                    }
                };

                ResolvedStatement::Declaration(ResolvedLocalVariable {
                    id: id,
                    debug_name: local.name.clone(),
                    mutable: local.mutable,
                    var_type: local.var_type.clone().map(|t| self.resolve_type(t)),
                    init,
                })
            }

            Statement::Assign {
                target,
                value,
            } => {
                // Walk through member/index chains to find the base variable for mutability check
                let base_var = Self::base_variable(target);
                match base_var {
                    Some(name) => match self.look_up_var(&name) {
                        Ok((_, m)) => {
                            if m == false {
                                self.errors.push(ResolveError::AssignToImmutable(name));
                            }
                        }
                        Err(e) => {
                            self.errors.push(e);
                        }
                    },
                    None => {
                        self.errors.push(ResolveError::InvalidAssignment);
                    }
                }

                ResolvedStatement::Assign {
                    target: self.resolve_expression(target),
                    value: self.resolve_expression(value),
                }
            }

            Statement::Return(expr) => ResolvedStatement::Return(expr.as_ref().map(|e| self.resolve_expression(e))),

            Statement::FunctionCall(expr) => ResolvedStatement::FunctionCall(self.resolve_expression(expr)),

            Statement::If {
                scopes,
                conditions,
            } => {
                let conditions: Vec<_> = conditions
                    .iter()
                    .map(|condition| self.resolve_expression(condition))
                    .collect();

                let scopes: Vec<_> = scopes.iter().map(|scope| self.resolve_scope(scope)).collect();

                ResolvedStatement::If {
                    scopes: scopes,
                    conditions: conditions,
                }
            }

            Statement::Loop { scope } => ResolvedStatement::Loop {
                scope: self.resolve_scope(scope),
            },
            Statement::While {
                condition,
                scope,
            } => ResolvedStatement::While {
                condition: self.resolve_expression(condition),
                scope: self.resolve_scope(scope),
            },
            Statement::Break => ResolvedStatement::Break,
            Statement::Continue => ResolvedStatement::Continue,
        };
    }

    fn resolve_expression(&mut self, expr: &Expression) -> ResolvedExpression {
        match expr {
            Expression::Variable(name) => match self.look_up_var(name) {
                Ok((id, _)) => ResolvedExpression::Variable(id),
                Err(e) => {
                    self.errors.push(e);
                    ResolvedExpression::Variable(Self::dummy_id())
                }
            },
            Expression::Binary {
                op,
                left,
                right,
            } => ResolvedExpression::Binary {
                op: *op,
                left: Box::new(self.resolve_expression(left)),
                right: Box::new(self.resolve_expression(right)),
            },
            Expression::Unary { op, expr } => ResolvedExpression::Unary {
                op: *op,
                expr: Box::new(self.resolve_expression(expr)),
            },
            Expression::Call {
                function,
                args,
            } => {
                let id = match self.function_names.get(function) {
                    Some(id) => *id,
                    None => {
                        self.errors.push(ResolveError::Undefined(function.clone()));
                        FunctionId {
                            id: u32::MAX,
                        }
                    }
                };
                let args = args.iter().map(|a| self.resolve_expression(a)).collect();
                ResolvedExpression::Call {
                    function: id,
                    args,
                }
            }
            Expression::Literal(l) => ResolvedExpression::Literal(*l),
            Expression::Member {
                object,
                field,
            } => ResolvedExpression::Member {
                base: Box::new(self.resolve_expression(object)),
                field: field.clone(),
            },
            Expression::Index {
                object,
                index,
            } => ResolvedExpression::Index {
                base: Box::new(self.resolve_expression(object)),
                index: Box::new(self.resolve_expression(index)),
            },
            Expression::Cast { ty, expr } => ResolvedExpression::Cast {
                ty: self.resolve_type(ty.clone()),
                expr: Box::new(self.resolve_expression(expr)),
            },
            Expression::StructLiteral {
                name,
                fields,
            } => {
                if self.struct_registry.contains_key(name) {
                    let resolved_fields: Vec<(String, ResolvedExpression)> = fields
                        .iter()
                        .map(|(fname, fexpr)| (fname.clone(), self.resolve_expression(fexpr)))
                        .collect();
                    ResolvedExpression::StructLiteral {
                        struct_name: name.clone(),
                        fields: resolved_fields,
                    }
                } else {
                    self.errors
                        .push(ResolveError::NotSupported(format!("unknown struct '{}'", name)));
                    ResolvedExpression::Literal(Literal::Bool(false))
                }
            }
            Expression::EnumLiteral {
                enum_name,
                variant,
                payload,
            } => {
                let variants = match self.enum_registry.get(enum_name) {
                    Some(v) => v,
                    None => {
                        self.errors
                            .push(ResolveError::NotSupported(format!("unknown enum '{}'", enum_name)));
                        return ResolvedExpression::Literal(Literal::Bool(false));
                    }
                };
                let (variant_index, variant_ty) = match variants.iter().position(|(n, _)| n == variant) {
                    Some(idx) => (idx as u32, variants[idx].1.clone()),
                    None => {
                        self.errors.push(ResolveError::NotSupported(format!(
                            "unknown variant '{}' for enum '{}'",
                            variant, enum_name
                        )));
                        return ResolvedExpression::Literal(Literal::Bool(false));
                    }
                };
                let resolved_payload = match (&variant_ty, payload) {
                    (Type::Void, Some(_)) => {
                        self.errors
                            .push(ResolveError::NotSupported(format!("variant '{}' has no payload", variant)));
                        return ResolvedExpression::Literal(Literal::Bool(false));
                    }
                    (Type::Void, None) => None,
                    (_, None) => {
                        self.errors
                            .push(ResolveError::NotSupported(format!("variant '{}' requires a payload", variant)));
                        return ResolvedExpression::Literal(Literal::Bool(false));
                    }
                    (_, Some(p)) => Some(Box::new(self.resolve_expression(p))),
                };
                ResolvedExpression::EnumLiteral {
                    enum_name: enum_name.clone(),
                    variant: variant.clone(),
                    variant_index,
                    payload: resolved_payload,
                }
            }
            Expression::ArrayDeclaration(arr) => match arr {
                ArrayDeclaration::Repeat { value, len } => {
                    let resolved_value = self.resolve_expression(value);
                    let resolved_len = self.resolve_expression(len);
                    let count = resolve_constant_count(&resolved_len).unwrap_or_else(|| {
                        self.errors
                            .push(ResolveError::NotSupported("array repeat count must be a constant integer".into()));
                        0
                    });
                    ResolvedExpression::ArrayLiteral(ResolvedArrayLiteral::Repeat {
                        value: Box::new(resolved_value),
                        count,
                    })
                }
                ArrayDeclaration::Normal { values } => {
                    let resolved: Vec<_> = values.iter().map(|v| self.resolve_expression(v)).collect();
                    ResolvedExpression::ArrayLiteral(ResolvedArrayLiteral::Normal {
                        values: resolved,
                    })
                }
            },
        }
    }
}

fn resolve_constant_count(expr: &ResolvedExpression) -> Option<u32> {
    match expr {
        ResolvedExpression::Literal(Literal::Uint(n)) => Some(*n as u32),
        ResolvedExpression::Literal(Literal::Int(n)) if *n >= 0 => Some(*n as u32),
        _ => None,
    }
}
