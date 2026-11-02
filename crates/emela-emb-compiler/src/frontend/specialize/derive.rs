//! Concrete trait helpers. Every structural field operation goes through the registry,
//! preserving custom implementations inside tuples, Option and derived nominal data.
use super::*;
use crate::frontend::{
    ast::Binary,
    traits::{BuiltinTrait, ParamBounds},
    types::{InvocationSets, Signature},
};

impl Specializer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn trait_target(
        &mut self,
        caller: &ir::Function,
        ty: b::TypeId,
        trait_id: usize,
        method: &str,
        method_arguments: &[b::TypeId],
        nested: bool,
        span: Span,
        depth: usize,
    ) -> Result<ir::DefinitionId> {
        if nested
            && method_arguments.is_empty()
            && ty == b::TypeId::STRING
            && method == "show"
            && self.registry.builtin_id(BuiltinTrait::Show) == Some(trait_id)
        {
            return self.builtin_helper(caller, BuiltinTrait::Show, ty, true, span, depth);
        }
        let resolved = match self.registry.resolve_generic(
            &self.core,
            ty,
            trait_id,
            method,
            method_arguments,
            &ParamBounds::new(),
        ) {
            Ok(resolved) => resolved,
            Err(error)
                if error.kind == crate::frontend::traits::TraitErrorKind::UnsatisfiedBound
                    && method == "show"
                    && self.registry.builtin_id(BuiltinTrait::Show) == Some(trait_id)
                    && self.debug_fallback.contains(&caller.id) =>
            {
                return self.debug_show_helper(caller, ty, nested, span, depth);
            }
            Err(error) => return Err(diagnostic(caller, span, error.message)),
        };
        if resolved.deferred {
            return Err(diagnostic(
                caller,
                span,
                "trait receiver is not concrete before backend lowering",
            ));
        }
        if let Some(definition) = resolved.definition {
            let template = self.templates.get(&definition).ok_or_else(|| {
                diagnostic(
                    caller,
                    span,
                    "trait implementation refers to an unknown function",
                )
            })?;
            let args = if template.type_parameters == 0 && method_arguments.is_empty() {
                vec![]
            } else {
                resolved.type_arguments
            };
            return self.instantiate(caller, definition, args, span, depth);
        }
        let kind = resolved.builtin.ok_or_else(|| {
            diagnostic(caller, span, "trait method has no concrete implementation")
        })?;
        self.builtin_helper(caller, kind, ty, nested, span, depth)
    }
    fn ordering_type(&self, function: &ir::Function) -> Result<b::TypeId> {
        let id = self.registry.builtin_id(BuiltinTrait::Ord).ok_or_else(|| {
            diagnostic(
                function,
                function.span,
                "compiler-owned Ord trait is missing",
            )
        })?;
        let definition = self
            .registry
            .trait_data(id)
            .map_err(|error| diagnostic(function, function.span, error.message))?;
        let signature = definition
            .methods
            .iter()
            .find(|method| method.name == "compare")
            .map(|method| method.signature)
            .ok_or_else(|| {
                diagnostic(function, function.span, "Ord.compare signature is missing")
            })?;
        match self.core.types.types.get(signature.index()) {
            Some(b::Type::Fn(signature)) => Ok(signature.ret),
            _ => Err(diagnostic(
                function,
                function.span,
                "Ord.compare does not have a function signature",
            )),
        }
    }
    fn builtin_helper(
        &mut self,
        caller: &ir::Function,
        kind: BuiltinTrait,
        ty: b::TypeId,
        nested: bool,
        span: Span,
        depth: usize,
    ) -> Result<ir::DefinitionId> {
        let nested = nested && ty == b::TypeId::STRING;
        let key = (kind, ty, nested);
        if let Some(&id) = self.builtin_helpers.get(&key) {
            return Ok(id);
        }
        if self.instances >= self.limits.instances || depth >= self.limits.depth {
            return Err(diagnostic(
                caller,
                span,
                "trait helper specialization limit exceeded",
            ));
        }
        let id = ir::DefinitionId(
            self.next_id
                .ok_or_else(|| diagnostic(caller, span, "definition ID namespace is exhausted"))?,
        );
        self.next_id = id.0.checked_add(1);
        self.instances += 1;
        let ret = match kind {
            BuiltinTrait::Eq => ValueType::Bool,
            BuiltinTrait::Show => ValueType::String,
            BuiltinTrait::Ord => frontend_type(self.ordering_type(caller)?),
        };
        let params = (0..if kind == BuiltinTrait::Show { 1 } else { 2 })
            .map(|index| ir::Local {
                id: ir::LocalId(index),
                name: format!("argument{index}"),
                ty: frontend_type(ty),
            })
            .collect::<Vec<_>>();
        let mut function = ir::Function {
            id,
            type_parameters: 0,
            immediate: false,
            constant: false,
            origin: None,
            canonical_const: None,
            derivation: None,
            error_show: None,
            name: format!("__trait_{kind:?}_{}_{}", ty.index(), id.0),
            source_name: caller.source_name.clone(),
            span,
            exported: false,
            export_symbol: None,
            signature: Signature {
                params: params.iter().map(|param| param.ty).collect(),
                ret: Some(ret),
                sets: InvocationSets::default(),
            },
            params,
            external: None,
            intrinsic: None,
            body: None,
        };
        self.builtin_helpers.insert(key, id);
        let slot = self.functions.len();
        self.functions.push(function.clone());
        function.body = Some(self.derive_body(&function, kind, nested, depth)?);
        self.function(&mut function, &[], depth + 1)?;
        self.functions[slot] = function;
        Ok(id)
    }
    pub(super) fn derive_body(
        &mut self,
        function: &ir::Function,
        kind: BuiltinTrait,
        nested: bool,
        depth: usize,
    ) -> Result<ir::Block> {
        let expected = if kind == BuiltinTrait::Show { 1 } else { 2 };
        if function.params.len() != expected {
            return Err(diagnostic(
                function,
                function.span,
                "structural trait helper has an invalid parameter count",
            ));
        }
        let ty = backend_type(function.params[0].ty);
        let node = self
            .core
            .types
            .types
            .get(ty.index())
            .cloned()
            .ok_or_else(|| {
                diagnostic(
                    function,
                    function.span,
                    "trait helper type is outside the catalog",
                )
            })?;
        if let b::Type::List(element) = node {
            if kind == BuiltinTrait::Show {
                return self.list_show_body(function, ty, element, depth);
            }
        }
        let mut builder = Builder {
            stage: self,
            function,
            next_local: function
                .params
                .iter()
                .map(|param| param.id.0)
                .max()
                .unwrap_or(0)
                + 1,
        };
        let left = builder.local(&function.params[0]);
        let right = function.params.get(1).map(|param| builder.local(param));
        let expression = match node {
            b::Type::List(element) => builder.list(kind, ty, element, left, right)?,
            b::Type::Tuple(fields) => builder.tuple(kind, ty, fields, left, right)?,
            b::Type::Enum(id, args) => builder.enumeration(kind, ty, id, args, left, right)?,
            b::Type::Int
            | b::Type::Int64
            | b::Type::Float
            | b::Type::Bool
            | b::Type::String
            | b::Type::Unit => builder.primitive(kind, ty, left, right, nested)?,
            b::Type::Fn(_) if kind == BuiltinTrait::Show => builder.literal("<fn>"),
            _ => {
                return Err(diagnostic(
                    function,
                    function.span,
                    "structural helper does not support this concrete value type yet",
                ));
            }
        };
        Ok(builder.block(expression))
    }
    fn list_show_body(
        &mut self,
        caller: &ir::Function,
        ty: b::TypeId,
        element: b::TypeId,
        depth: usize,
    ) -> Result<ir::Block> {
        if self.instances >= self.limits.instances || depth >= self.limits.depth {
            return Err(diagnostic(
                caller,
                caller.span,
                "List Show helper specialization limit exceeded",
            ));
        }
        let id = ir::DefinitionId(self.next_id.ok_or_else(|| {
            diagnostic(caller, caller.span, "definition ID namespace is exhausted")
        })?);
        self.next_id = id.0.checked_add(1);
        self.instances += 1;
        let mut helper = caller.clone();
        helper.id = id;
        helper.name = format!("__list_show_{}", id.0);
        helper.derivation = None;
        helper.error_show = None;
        helper.immediate = false;
        helper.type_parameters = 0;
        helper.constant = false;
        helper.origin = None;
        helper.exported = false;
        helper.export_symbol = None;
        helper.external = None;
        helper.intrinsic = None;
        helper.params = [ty, b::TypeId::STRING, b::TypeId::STRING]
            .into_iter()
            .enumerate()
            .map(|(index, ty)| ir::Local {
                id: ir::LocalId(index),
                name: format!("part{index}"),
                ty: frontend_type(ty),
            })
            .collect();
        helper.signature = Signature {
            params: helper.params.iter().map(|p| p.ty).collect(),
            ret: Some(ValueType::String),
            sets: InvocationSets::default(),
        };
        let mut builder = Builder {
            stage: self,
            function: &helper,
            next_local: 3,
        };
        let head = builder.fresh(element);
        let tail = builder.fresh(ty);
        let nil = builder.list_nil(frontend_type(ty));
        let cons = builder.list_cons(
            frontend_type(ty),
            builder.binding(head.clone()),
            builder.binding(tail.clone()),
        );
        let empty = builder.expression(
            ir::ExprKind::Concat(vec![builder.local(&helper.params[1]), builder.literal("]")]),
            ValueType::String,
        );
        let shown = builder.trait_call(
            BuiltinTrait::Show,
            element,
            vec![builder.local(&head)],
            true,
        )?;
        let accumulated = builder.expression(
            ir::ExprKind::Concat(vec![
                builder.local(&helper.params[1]),
                builder.local(&helper.params[2]),
                shown,
            ]),
            ValueType::String,
        );
        let recur = builder.call(
            id,
            vec![builder.local(&tail), accumulated, builder.literal(", ")],
            ValueType::String,
        );
        let body = builder.matching(
            builder.local(&helper.params[0]),
            vec![builder.arm(nil, empty), builder.arm(cons, recur)],
            ValueType::String,
        );
        helper.body = Some(builder.block(body));
        if self.debug_fallback.contains(&caller.id) {
            self.debug_fallback.insert(id);
        }
        self.templates.insert(id, helper.clone());
        let slot = self.functions.len();
        self.functions.push(helper.clone());
        self.function(&mut helper, &[], depth + 1)?;
        self.functions[slot] = helper;
        let builder = Builder {
            stage: self,
            function: caller,
            next_local: 0,
        };
        Ok(builder.block(builder.call(
            id,
            vec![
                builder.local(&caller.params[0]),
                builder.literal("["),
                builder.literal(""),
            ],
            ValueType::String,
        )))
    }
}
struct Builder<'a> {
    stage: &'a mut Specializer,
    function: &'a ir::Function,
    next_local: usize,
}
impl Builder<'_> {
    fn expression(&self, kind: ir::ExprKind, ty: ValueType) -> ir::Expr {
        ir::Expr {
            kind,
            ty,
            span: self.function.span,
        }
    }
    fn block(&self, expression: ir::Expr) -> ir::Block {
        ir::Block {
            ty: expression.ty,
            statements: vec![ir::Statement::Expr(expression)],
            span: self.function.span,
        }
    }
    fn local(&self, local: &ir::Local) -> ir::Expr {
        self.expression(ir::ExprKind::Local(local.id), local.ty)
    }
    fn literal(&self, text: &str) -> ir::Expr {
        self.expression(
            ir::ExprKind::Literal(ir::Literal::String(text.into())),
            ValueType::String,
        )
    }
    fn boolean(&self, value: bool) -> ir::Expr {
        self.expression(
            ir::ExprKind::Literal(ir::Literal::Bool(value)),
            ValueType::Bool,
        )
    }
    fn binary(&self, op: Binary, left: ir::Expr, right: ir::Expr) -> ir::Expr {
        self.expression(
            ir::ExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            ValueType::Bool,
        )
    }
    fn conditional(&self, condition: ir::Expr, then: ir::Expr, else_: ir::Expr) -> ir::Expr {
        let ty = then.ty;
        self.expression(
            ir::ExprKind::If {
                condition: Box::new(condition),
                then: self.block(then),
                else_: self.block(else_),
            },
            ty,
        )
    }
    fn fresh(&mut self, ty: b::TypeId) -> ir::Local {
        let local = ir::Local {
            id: ir::LocalId(self.next_local),
            name: format!("field{}", self.next_local),
            ty: frontend_type(ty),
        };
        self.next_local += 1;
        local
    }
    fn any(&self, ty: ValueType) -> ir::Pattern {
        ir::Pattern {
            kind: ir::PatternKind::Any,
            ty,
            span: self.function.span,
        }
    }
    fn binding(&self, local: ir::Local) -> ir::Pattern {
        ir::Pattern {
            ty: local.ty,
            kind: ir::PatternKind::Bind(local),
            span: self.function.span,
        }
    }
    fn list_nil(&self, ty: ValueType) -> ir::Pattern {
        ir::Pattern {
            kind: ir::PatternKind::ListNil,
            ty,
            span: self.function.span,
        }
    }
    fn list_cons(&self, ty: ValueType, head: ir::Pattern, tail: ir::Pattern) -> ir::Pattern {
        ir::Pattern {
            kind: ir::PatternKind::ListCons {
                head: Box::new(head),
                tail: Box::new(tail),
            },
            ty,
            span: self.function.span,
        }
    }
    fn call(&self, target: ir::DefinitionId, args: Vec<ir::Expr>, ty: ValueType) -> ir::Expr {
        let order = (0..args.len()).collect();
        self.expression(
            ir::ExprKind::Call {
                target,
                args,
                order,
                type_arguments: vec![],
            },
            ty,
        )
    }
    fn list(
        &mut self,
        kind: BuiltinTrait,
        ty: b::TypeId,
        element: b::TypeId,
        left: ir::Expr,
        right: Option<ir::Expr>,
    ) -> Result<ir::Expr> {
        let right = right.expect("List comparison arity checked");
        let left_head = self.fresh(element);
        let right_head = self.fresh(element);
        let left_tail = self.fresh(ty);
        let right_tail = self.fresh(ty);
        let pair_type = frontend_type(self.stage.core.types.intern(b::Type::Tuple(vec![ty, ty])));
        let nil = self.list_nil(frontend_type(ty));
        let left_cons = self.list_cons(
            frontend_type(ty),
            self.binding(left_head.clone()),
            self.binding(left_tail.clone()),
        );
        let right_cons = self.list_cons(
            frontend_type(ty),
            self.binding(right_head.clone()),
            self.binding(right_tail.clone()),
        );
        let heads = self.trait_call(
            kind,
            element,
            vec![self.local(&left_head), self.local(&right_head)],
            false,
        )?;
        let tails = self.trait_call(
            kind,
            ty,
            vec![self.local(&left_tail), self.local(&right_tail)],
            false,
        )?;
        let mut arms = Vec::new();
        let both_empty = self.tuple_pattern(pair_type, vec![nil.clone(), nil.clone()]);
        let both_cons = self.tuple_pattern(pair_type, vec![left_cons, right_cons]);
        let ret = if kind == BuiltinTrait::Eq {
            arms.push(self.arm(both_empty, self.boolean(true)));
            arms.push(self.arm(both_cons, self.binary(Binary::And, heads, tails)));
            arms.push(self.arm(self.any(pair_type), self.boolean(false)));
            ValueType::Bool
        } else {
            let equal = self.ordering("Equal")?;
            let ordering_type = backend_type(equal.ty);
            let b::Type::Enum(ordering_id, _) = self.stage.core.types[ordering_type] else {
                unreachable!("Ordering checked")
            };
            let equal_variant = self.stage.core.enums[ordering_id]
                .variants
                .iter()
                .position(|v| v.name == "Equal")
                .expect("Ordering checked");
            let other = self.fresh(ordering_type);
            let compared = self.matching(
                heads,
                vec![
                    self.arm(
                        self.constructor_pattern(equal.ty, ordering_id, equal_variant, vec![]),
                        tails,
                    ),
                    self.arm(self.binding(other.clone()), self.local(&other)),
                ],
                equal.ty,
            );
            arms.push(self.arm(both_empty, equal.clone()));
            arms.push(self.arm(
                self.tuple_pattern(pair_type, vec![nil.clone(), self.any(frontend_type(ty))]),
                self.ordering("Less")?,
            ));
            arms.push(self.arm(
                self.tuple_pattern(pair_type, vec![self.any(frontend_type(ty)), nil]),
                self.ordering("Greater")?,
            ));
            arms.push(self.arm(both_cons, compared));
            equal.ty
        };
        Ok(self.matching(
            self.expression(ir::ExprKind::Tuple(vec![left, right]), pair_type),
            arms,
            ret,
        ))
    }
    fn tuple_pattern(&self, ty: ValueType, fields: Vec<ir::Pattern>) -> ir::Pattern {
        ir::Pattern {
            kind: ir::PatternKind::Tuple(fields),
            ty,
            span: self.function.span,
        }
    }
    fn constructor_pattern(
        &self,
        ty: ValueType,
        id: b::EnumId,
        variant: usize,
        fields: Vec<ir::Pattern>,
    ) -> ir::Pattern {
        ir::Pattern {
            kind: ir::PatternKind::Constructor {
                ctor: b::CtorRef {
                    enum_id: id,
                    variant,
                },
                fields,
            },
            ty,
            span: self.function.span,
        }
    }
    fn matching(&self, value: ir::Expr, arms: Vec<ir::Arm>, ty: ValueType) -> ir::Expr {
        self.expression(
            ir::ExprKind::Match {
                value: Box::new(value),
                arms,
            },
            ty,
        )
    }
    fn arm(&self, pattern: ir::Pattern, value: ir::Expr) -> ir::Arm {
        ir::Arm {
            pattern,
            guard: None,
            value,
        }
    }
    fn trait_call(
        &self,
        kind: BuiltinTrait,
        ty: b::TypeId,
        args: Vec<ir::Expr>,
        nested: bool,
    ) -> Result<ir::Expr> {
        let id = self.stage.registry.builtin_id(kind).ok_or_else(|| {
            diagnostic(
                self.function,
                self.function.span,
                format!("compiler-owned {kind:?} trait is missing"),
            )
        })?;
        let ret = match kind {
            BuiltinTrait::Eq => ValueType::Bool,
            BuiltinTrait::Show => ValueType::String,
            BuiltinTrait::Ord => frontend_type(self.stage.ordering_type(self.function)?),
        };
        let method = match kind {
            BuiltinTrait::Eq => "eq",
            BuiltinTrait::Show => "show",
            BuiltinTrait::Ord => "compare",
        };
        let order = (0..args.len()).collect();
        Ok(self.expression(
            ir::ExprKind::TraitCall {
                trait_id: id,
                method: method.into(),
                self_type: ty,
                args,
                order,
                method_type_arguments: vec![],
                nested_show: nested,
            },
            ret,
        ))
    }
    fn ordering(&self, name: &str) -> Result<ir::Expr> {
        let ty = self.stage.ordering_type(self.function)?;
        let Some(b::Type::Enum(id, _)) = self.stage.core.types.types.get(ty.index()) else {
            return Err(diagnostic(
                self.function,
                self.function.span,
                "Ord.compare result is not an Ordering enum",
            ));
        };
        let definition = self
            .stage
            .core
            .enums
            .iter()
            .find_map(|(key, definition)| (key == *id).then_some(definition))
            .ok_or_else(|| {
                diagnostic(
                    self.function,
                    self.function.span,
                    "Ordering enum definition is missing",
                )
            })?;
        let variant = definition
            .variants
            .iter()
            .position(|variant| variant.name == name && variant.tys.is_empty())
            .ok_or_else(|| {
                diagnostic(
                    self.function,
                    self.function.span,
                    format!("Ordering has no unit variant `{name}`"),
                )
            })?;
        Ok(self.expression(
            ir::ExprKind::Make {
                ctor: b::CtorRef {
                    enum_id: *id,
                    variant,
                },
                args: vec![],
                order: vec![],
            },
            frontend_type(ty),
        ))
    }
    fn primitive(
        &mut self,
        kind: BuiltinTrait,
        ty: b::TypeId,
        left: ir::Expr,
        right: Option<ir::Expr>,
        nested: bool,
    ) -> Result<ir::Expr> {
        Ok(match kind {
            BuiltinTrait::Eq => {
                if ty == b::TypeId::UNIT {
                    self.boolean(true)
                } else {
                    self.binary(Binary::Eq, left, right.expect("Eq arity checked"))
                }
            }
            BuiltinTrait::Ord => {
                if ty == b::TypeId::FLOAT {
                    return Err(diagnostic(
                        self.function,
                        self.function.span,
                        "Float has no Ord instance",
                    ));
                }
                if ty == b::TypeId::UNIT {
                    return self.ordering("Equal");
                }
                let right = right.expect("Ord arity checked");
                let equal = self.ordering("Equal")?;
                let less = self.ordering("Less")?;
                let greater = self.ordering("Greater")?;
                if ty == b::TypeId::BOOL {
                    self.conditional(
                        self.binary(Binary::Eq, left.clone(), right),
                        equal,
                        self.conditional(left, greater, less),
                    )
                } else {
                    self.conditional(
                        self.binary(Binary::Lt, left.clone(), right.clone()),
                        less,
                        self.conditional(self.binary(Binary::Gt, left, right), greater, equal),
                    )
                }
            }
            BuiltinTrait::Show => match ty {
                b::TypeId::UNIT => self.literal("()"),
                b::TypeId::BOOL => {
                    self.conditional(left, self.literal("True"), self.literal("False"))
                }
                b::TypeId::STRING if !nested => left,
                _ => {
                    let op = match ty {
                        b::TypeId::INT => b::Builtin::IntToString,
                        b::TypeId::INT64 => b::Builtin::Int64ToString,
                        b::TypeId::FLOAT => b::Builtin::FloatToString,
                        b::TypeId::STRING => b::Builtin::ShowNested,
                        _ => {
                            return Err(diagnostic(
                                self.function,
                                self.function.span,
                                "unsupported primitive Show receiver",
                            ));
                        }
                    };
                    self.expression(
                        ir::ExprKind::Builtin {
                            op,
                            args: vec![left],
                        },
                        ValueType::String,
                    )
                }
            },
        })
    }
    fn compare_fields(
        &mut self,
        kind: BuiltinTrait,
        fields: &[b::TypeId],
        left: &[ir::Local],
        right: &[ir::Local],
    ) -> Result<ir::Expr> {
        if kind == BuiltinTrait::Eq {
            let mut result = self.boolean(true);
            for ((&ty, left), right) in fields.iter().zip(left).zip(right).rev() {
                let compare =
                    self.trait_call(kind, ty, vec![self.local(left), self.local(right)], false)?;
                result = self.binary(Binary::And, compare, result);
            }
            Ok(result)
        } else {
            let mut result = self.ordering("Equal")?;
            let ordering_ty = backend_type(result.ty);
            let b::Type::Enum(ordering_id, _) = self.stage.core.types[ordering_ty].clone() else {
                unreachable!("Ordering checked")
            };
            let equal_variant = self.stage.core.enums[ordering_id]
                .variants
                .iter()
                .position(|variant| variant.name == "Equal")
                .expect("Ordering checked");
            for ((&ty, left), right) in fields.iter().zip(left).zip(right).rev() {
                let compare =
                    self.trait_call(kind, ty, vec![self.local(left), self.local(right)], false)?;
                let other = self.fresh(ordering_ty);
                let equal = self.constructor_pattern(result.ty, ordering_id, equal_variant, vec![]);
                let fallback = self.binding(other.clone());
                result = self.matching(
                    compare,
                    vec![
                        self.arm(equal, result),
                        self.arm(fallback, self.local(&other)),
                    ],
                    frontend_type(ordering_ty),
                );
            }
            Ok(result)
        }
    }
    fn show_fields(
        &self,
        prefix: &str,
        suffix: &str,
        names: Option<&[String]>,
        fields: &[b::TypeId],
        values: &[ir::Local],
    ) -> Result<ir::Expr> {
        let mut parts = vec![self.literal(prefix)];
        for (index, (&ty, value)) in fields.iter().zip(values).enumerate() {
            if index != 0 {
                parts.push(self.literal(", "));
            }
            if let Some(names) = names {
                parts.push(self.literal(&format!("{}: ", names[index])));
            }
            parts.push(self.trait_call(BuiltinTrait::Show, ty, vec![self.local(value)], true)?);
        }
        parts.push(self.literal(suffix));
        Ok(self.expression(ir::ExprKind::Concat(parts), ValueType::String))
    }
    fn tuple(
        &mut self,
        kind: BuiltinTrait,
        ty: b::TypeId,
        fields: Vec<b::TypeId>,
        left: ir::Expr,
        right: Option<ir::Expr>,
    ) -> Result<ir::Expr> {
        let left_fields: Vec<_> = fields.iter().map(|&ty| self.fresh(ty)).collect();
        let left_pattern = self.tuple_pattern(
            frontend_type(ty),
            left_fields
                .iter()
                .cloned()
                .map(|local| self.binding(local))
                .collect(),
        );
        if kind == BuiltinTrait::Show {
            let value = self.show_fields("(", ")", None, &fields, &left_fields)?;
            return Ok(self.matching(left, vec![self.arm(left_pattern, value)], ValueType::String));
        }
        let right_fields: Vec<_> = fields.iter().map(|&ty| self.fresh(ty)).collect();
        let right_pattern = self.tuple_pattern(
            frontend_type(ty),
            right_fields
                .iter()
                .cloned()
                .map(|local| self.binding(local))
                .collect(),
        );
        let pair_ty = self.stage.core.types.intern(b::Type::Tuple(vec![ty, ty]));
        let pair = self.expression(
            ir::ExprKind::Tuple(vec![left, right.expect("comparison arity checked")]),
            frontend_type(pair_ty),
        );
        let pattern = self.tuple_pattern(pair.ty, vec![left_pattern, right_pattern]);
        let value = self.compare_fields(kind, &fields, &left_fields, &right_fields)?;
        let ret = value.ty;
        Ok(self.matching(pair, vec![self.arm(pattern, value)], ret))
    }
    fn enumeration(
        &mut self,
        kind: BuiltinTrait,
        ty: b::TypeId,
        id: b::EnumId,
        args: Vec<b::TypeId>,
        left: ir::Expr,
        right: Option<ir::Expr>,
    ) -> Result<ir::Expr> {
        let definition = self
            .stage
            .core
            .enums
            .iter()
            .find_map(|(key, value)| (key == id).then_some(value))
            .cloned()
            .ok_or_else(|| {
                diagnostic(
                    self.function,
                    self.function.span,
                    "derived enum definition is missing",
                )
            })?;
        let pair_ty = if kind != BuiltinTrait::Show {
            Some(self.stage.core.types.intern(b::Type::Tuple(vec![ty, ty])))
        } else {
            None
        };
        let mut arms = Vec::new();
        for (variant, definition) in definition.variants.iter().enumerate() {
            let fields = definition
                .tys
                .iter()
                .map(|&field| {
                    self.stage.substitute(
                        self.function,
                        field,
                        &args,
                        self.function.span,
                        &mut HashMap::new(),
                        &mut HashSet::new(),
                        0,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let left_fields: Vec<_> = fields.iter().map(|&ty| self.fresh(ty)).collect();
            let left_pattern = self.constructor_pattern(
                frontend_type(ty),
                id,
                variant,
                left_fields
                    .iter()
                    .cloned()
                    .map(|local| self.binding(local))
                    .collect(),
            );
            let (pattern, value) = if kind == BuiltinTrait::Show {
                let value = if fields.is_empty() {
                    self.literal(&definition.name)
                } else {
                    let names = match &definition.fields {
                        b::VariantFields::Named(names) => Some(names.as_slice()),
                        _ => None,
                    };
                    self.show_fields(
                        &format!("{}(", definition.name),
                        ")",
                        names,
                        &fields,
                        &left_fields,
                    )?
                };
                (left_pattern, value)
            } else {
                let right_fields: Vec<_> = fields.iter().map(|&ty| self.fresh(ty)).collect();
                let right_pattern = self.constructor_pattern(
                    frontend_type(ty),
                    id,
                    variant,
                    right_fields
                        .iter()
                        .cloned()
                        .map(|local| self.binding(local))
                        .collect(),
                );
                let pattern = self.tuple_pattern(
                    frontend_type(pair_ty.expect("comparison pair")),
                    vec![left_pattern, right_pattern],
                );
                let value = self.compare_fields(kind, &fields, &left_fields, &right_fields)?;
                (pattern, value)
            };
            arms.push(self.arm(pattern, value));
        }
        if kind == BuiltinTrait::Show {
            return Ok(self.matching(left, arms, ValueType::String));
        }
        let right = right.expect("comparison arity checked");
        let pair = self.expression(
            ir::ExprKind::Tuple(vec![left.clone(), right.clone()]),
            frontend_type(pair_ty.expect("comparison pair")),
        );
        if kind == BuiltinTrait::Eq {
            arms.push(self.arm(self.any(pair.ty), self.boolean(false)));
            return Ok(self.matching(pair, arms, ValueType::Bool));
        }
        let equal = self.ordering("Equal")?;
        let compared = self.matching(pair, arms, equal.ty);
        let left_tag = self.expression(ir::ExprKind::Tag(Box::new(left)), ValueType::Int);
        let right_tag = self.expression(ir::ExprKind::Tag(Box::new(right)), ValueType::Int);
        Ok(self.conditional(
            self.binary(Binary::Lt, left_tag.clone(), right_tag.clone()),
            self.ordering("Less")?,
            self.conditional(
                self.binary(Binary::Gt, left_tag, right_tag),
                self.ordering("Greater")?,
                compared,
            ),
        ))
    }
}
