//! Expand providers into sequential acquisitions, preserving lexical capability lifetimes.
use super::*;
use crate::contract::ir::{EffectId, Type, TypeId};

#[derive(Clone)]
enum Supplier {
    Handler {
        id: usize,
        module: usize,
        args: Option<Vec<ast::Argument>>,
        layer: bool,
    },
    Existing(EffectId),
}
#[derive(Clone)]
struct Entry {
    effect: EffectId,
    supplier: Supplier,
    span: Span,
}

impl Checker {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn with_handlers(
        &mut self,
        module: usize,
        items: &[ast::WithItem],
        body: &ast::Block,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let saved_names = locals.names.clone();
        let result = self.with_handlers_inner(module, items, body, locals, expected, span);
        locals.names = saved_names;
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn with_handlers_inner(
        &mut self,
        module: usize,
        items: &[ast::WithItem],
        body: &ast::Block,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let mut entries = Vec::new();
        let mut labels = HashSet::new();
        for item in items {
            self.with_item(module, item, &mut entries, &mut labels)?;
        }
        let retained: Vec<_> = entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                (!entries[index + 1..]
                    .iter()
                    .any(|later| later.effect == entry.effect))
                .then_some(index)
            })
            .collect();
        // Overridden arguments are still resolved and checked, but never enter the runtime IR.
        let mut initializers = HashMap::new();
        for (index, entry) in entries.iter().enumerate() {
            if let Supplier::Handler {
                id,
                module: argument_module,
                args,
                layer,
            } = &entry.supplier
            {
                let info = self.effects.handlers[*id].clone();
                let init = info.init.expect("bound handler initializer");
                self.function(init)?;
                if args.is_some() && self.definitions[init.0].ast.params.is_empty() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        entry.span,
                        "a handler whose init takes no parameters cannot be given arguments",
                    ));
                }
                let mut layer_locals = Locals {
                    names: HashMap::new(),
                    next: locals.next,
                };
                let call = if *layer {
                    // Layer expressions belong to the declaration, including the absence
                    // of caller locals, type parameters and initializer privileges.
                    let saved_parameters = std::mem::take(&mut self.current_parameters);
                    let saved_bounds = std::mem::take(&mut self.current_bounds);
                    let saved_self = self.current_self_type.take();
                    let saved_method = self.current_effect_method.take();
                    let call = self.with_init_call(
                        *argument_module,
                        init,
                        args.as_deref().unwrap_or(&[]),
                        &mut layer_locals,
                        entry.span,
                    );
                    self.current_parameters = saved_parameters;
                    self.current_bounds = saved_bounds;
                    self.current_self_type = saved_self;
                    self.current_effect_method = saved_method;
                    call?
                } else {
                    self.with_init_call(
                        *argument_module,
                        init,
                        args.as_deref().unwrap_or(&[]),
                        locals,
                        entry.span,
                    )?
                };
                if *layer {
                    locals.next = layer_locals.next;
                    if let ir::ExprKind::Call { args, .. } = &call.kind {
                        for argument in args {
                            if !self.known_errors(*argument_module, argument)?.is_empty()
                                || !self
                                    .with_expression_effects(*argument_module, argument)?
                                    .is_empty()
                            {
                                return Err(self.error(
                                    *argument_module,
                                    DK::Type,
                                    argument.span,
                                    "layer initializer arguments must be pure and infallible",
                                ));
                            }
                        }
                    }
                }
                initializers.insert(index, call);
            }
        }
        let mut bindings = Vec::new();
        for &index in &retained {
            if let Some(initializer) = initializers.get_mut(&index) {
                let ir::ExprKind::Call { args, .. } = &mut initializer.kind else {
                    unreachable!("checked initializer call")
                };
                for argument in args {
                    let local = locals.bind(format!("$with_argument_{}", locals.next), argument.ty);
                    let value = std::mem::replace(argument, with_reference(&local, argument.span));
                    bindings.push(ir::WithBinding {
                        local,
                        value,
                        effects: vec![],
                        cleanup: None,
                        retire: None,
                    });
                }
            }
        }
        let mut dependencies = HashMap::new();
        for &index in &retained {
            let needed = match entries[index].supplier.clone() {
                Supplier::Existing(effect) => vec![effect],
                Supplier::Handler { id, .. } => {
                    self.with_handler_requirements(module, id, entries[index].span)?
                }
            };
            dependencies.insert(index, needed);
        }
        let mut ordered = Vec::new();
        let mut pending = retained.clone();
        while !pending.is_empty() {
            let ready = pending.iter().position(|&index| {
                dependencies[&index].iter().all(|&needed| {
                    needed == entries[index].effect
                        || !pending
                            .iter()
                            .any(|&other| other != index && entries[other].effect == needed)
                })
            });
            let Some(ready) = ready else {
                return Err(self.error(
                    module,
                    DK::Type,
                    entries[pending[0]].span,
                    "cyclic handler requirements in with",
                ));
            };
            ordered.push(pending.remove(ready));
        }
        let mut provided = Vec::new();
        for index in ordered {
            let entry = &entries[index];
            let (value, cleanup, retire) = match &entry.supplier {
                Supplier::Existing(effect) => {
                    let ty =
                        data::frontend_type(self.catalog.core.types.intern(Type::Handler(*effect)));
                    (
                        ir::Expr {
                            kind: ir::ExprKind::Use { effect: *effect },
                            ty,
                            span: entry.span,
                        },
                        None,
                        None,
                    )
                }
                Supplier::Handler { id, .. } => {
                    let info = self.effects.handlers[*id].clone();
                    let stateful = self.modules.modules[info.module]
                        .ast
                        .handlers
                        .iter()
                        .find(|handler| handler.name.value == info.name)
                        .is_some_and(|handler| {
                            handler.fields.is_some()
                                || handler.init.is_some()
                                || handler.release.is_some()
                        });
                    let stateless_value = (!stateful).then(|| {
                        let body = self.checked[info.init.unwrap().0]
                            .as_ref()
                            .unwrap()
                            .body
                            .clone()
                            .unwrap();
                        ir::Expr {
                            ty: data::frontend_type(info.state_type),
                            span: entry.span,
                            kind: ir::ExprKind::Block(body),
                        }
                    });
                    let state = locals.bind(
                        format!("$handler_state_{}", locals.next),
                        data::frontend_type(info.state_type),
                    );
                    bindings.push(ir::WithBinding {
                        local: state.clone(),
                        value: initializers.remove(&index).expect("checked initializer"),
                        effects: vec![],
                        cleanup: None,
                        retire: None,
                    });
                    let mut operations = Vec::new();
                    for &target in &info.operations {
                        let (signature, type_arguments) =
                            self.function_instance(module, target, entry.span)?;
                        let params = signature
                            .params
                            .iter()
                            .skip(1)
                            .enumerate()
                            .map(|(index, &ty)| {
                                locals.bind(
                                    format!("$operation_parameter_{index}_{}", locals.next),
                                    ty,
                                )
                            })
                            .collect::<Vec<_>>();
                        let ret = signature.ret.expect("checked operation result");
                        let arguments = std::iter::once(
                            stateless_value
                                .clone()
                                .unwrap_or_else(|| with_reference(&state, entry.span)),
                        )
                        .chain(params.iter().map(|local| with_reference(local, entry.span)))
                        .collect::<Vec<_>>();
                        let call = ir::Expr {
                            kind: ir::ExprKind::Call {
                                target,
                                type_arguments,
                                order: (0..arguments.len()).collect(),
                                args: arguments,
                            },
                            ty: ret,
                            span: entry.span,
                        };
                        let callable = Signature {
                            params: params.iter().map(|param| param.ty).collect(),
                            ret: Some(ret),
                            sets: InvocationSets {
                                effects: vec![],
                                ..signature.sets
                            },
                        };
                        let ty = self.signature_type(&callable, ret);
                        operations.push(ir::Expr {
                            kind: ir::ExprKind::Lambda {
                                params,
                                captures: if stateful { vec![state.id] } else { vec![] },
                                body: ir::Block {
                                    statements: vec![ir::Statement::Expr(call)],
                                    ty: ret,
                                    span: entry.span,
                                },
                            },
                            ty,
                            span: entry.span,
                        });
                    }
                    let cleanup = info
                        .release
                        .map(|target| {
                            let (signature, type_arguments) =
                                self.function_instance(module, target, entry.span)?;
                            Ok(ir::Expr {
                                kind: ir::ExprKind::Call {
                                    target,
                                    type_arguments,
                                    args: vec![with_reference(&state, entry.span)],
                                    order: vec![0],
                                },
                                ty: signature.ret.unwrap_or(Ty::Unit),
                                span: entry.span,
                            })
                        })
                        .transpose()?;
                    let ty = data::frontend_type(
                        self.catalog.core.types.intern(Type::Handler(info.effect)),
                    );
                    (
                        ir::Expr {
                            kind: ir::ExprKind::Handler {
                                name: info.name.clone(),
                                effect: info.effect,
                                operations,
                            },
                            ty,
                            span: entry.span,
                        },
                        cleanup,
                        stateful.then_some(info.name),
                    )
                }
            };
            let local = locals.bind(format!("$with_provider_{}", locals.next), value.ty);
            provided.push((entry.effect, local.clone()));
            bindings.push(ir::WithBinding {
                local,
                value,
                effects: vec![entry.effect],
                cleanup,
                retire,
            });
        }
        let mut body = self.block(module, body, locals, expected)?;
        let body_expression = ir::Expr {
            kind: ir::ExprKind::Block(body.clone()),
            ty: body.ty,
            span: body.span,
        };
        let required = self.with_expression_effects(module, &body_expression)?;
        let mut required_effects = HashSet::new();
        self.with_effect_members(module, &required, span, &mut required_effects)?;
        let defaults = self
            .catalog
            .effects
            .declarations
            .iter()
            .filter(|declaration| declaration.ast.default)
            .map(|declaration| declaration.id)
            .collect::<Vec<_>>();
        for label in defaults {
            if provided.iter().any(|(effect, _)| *effect == label)
                || !required_effects.contains(&label)
            {
                continue;
            }
            let Some(base) = self.catalog.core.effects[label].label_of else {
                continue;
            };
            let Some((_, provider)) = provided.iter().find(|(effect, _)| *effect == base).cloned()
            else {
                continue;
            };
            let local = locals.bind(format!("$default_provider_{}", locals.next), provider.ty);
            bindings.push(ir::WithBinding {
                local: local.clone(),
                value: with_reference(&provider, span),
                effects: vec![label],
                cleanup: None,
                retire: None,
            });
            provided.push((label, local));
        }
        self.with_lexical_closures(module, &mut body, &provided)?;
        Ok(ir::Expr {
            ty: body.ty,
            kind: ir::ExprKind::With { bindings, body },
            span,
        })
    }

    fn with_item(
        &mut self,
        module: usize,
        item: &ast::WithItem,
        entries: &mut Vec<Entry>,
        labels: &mut HashSet<EffectId>,
    ) -> Result<(), Diagnostic> {
        if let Some(provider) = &item.by {
            let label = self.catalog.resolve_effect(
                &self.modules,
                module,
                &item.name,
                &self.current_parameters,
            )?;
            let Some(base) = self.catalog.core.effects[label].label_of else {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.name.span,
                    "the left side of by must be an effect label",
                ));
            };
            if item.args.is_some() {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.span,
                    "an effect label cannot take initializer arguments",
                ));
            }
            if !labels.insert(label) {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.span,
                    "the same label cannot have two explicit providers in one with",
                ));
            }
            let supplier = if let Some(id) = self.effects.find_handler(module, &provider.name.value)
            {
                if self.effects.handlers[id].effect != base {
                    return Err(self.error(
                        module,
                        DK::Type,
                        provider.span,
                        "label provider handles a different underlying effect",
                    ));
                }
                Supplier::Handler {
                    id,
                    module,
                    args: provider.args.clone(),
                    layer: false,
                }
            } else {
                if provider.args.is_some() {
                    return Err(self.error(
                        module,
                        DK::Type,
                        provider.span,
                        "an effect provider cannot take initializer arguments",
                    ));
                }
                let effect = self.catalog.resolve_effect(
                    &self.modules,
                    module,
                    &provider.name,
                    &self.current_parameters,
                )?;
                if crate::semantics::effect_base(&self.catalog.core, effect) != Some(base) {
                    return Err(self.error(
                        module,
                        DK::Type,
                        provider.span,
                        "label provider serves a different underlying effect",
                    ));
                }
                Supplier::Existing(effect)
            };
            entries.push(Entry {
                effect: label,
                supplier,
                span: item.span,
            });
        } else if let Some(id) = self.effects.find_handler(module, &item.name.value) {
            entries.push(Entry {
                effect: self.effects.handlers[id].effect,
                supplier: Supplier::Handler {
                    id,
                    module,
                    args: item.args.clone(),
                    layer: false,
                },
                span: item.span,
            });
        } else if let Some(id) = self.effects.find_layer(module, &item.name.value) {
            if item.args.is_some() {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.span,
                    "a layer cannot take initializer arguments",
                ));
            }
            for member in self.effects.layers[id].items.clone() {
                entries.push(Entry {
                    effect: self.effects.handlers[member.handler].effect,
                    supplier: Supplier::Handler {
                        id: member.handler,
                        module: member.module,
                        args: member.args,
                        layer: true,
                    },
                    span: member.span,
                });
            }
        } else {
            let label = self.catalog.resolve_effect(
                &self.modules,
                module,
                &item.name,
                &self.current_parameters,
            )?;
            let Some(base) = self.catalog.core.effects[label].label_of else {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.span,
                    "only effect labels may be written as providers without a handler",
                ));
            };
            if item.args.is_some() {
                return Err(self.error(
                    module,
                    DK::Type,
                    item.span,
                    "an effect label cannot take initializer arguments",
                ));
            }
            entries.push(Entry {
                effect: base,
                supplier: Supplier::Existing(label),
                span: item.span,
            });
        }
        Ok(())
    }
    fn with_init_call(
        &mut self,
        module: usize,
        target: DefinitionId,
        args: &[ast::Argument],
        locals: &mut Locals,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let name = format!("$with_initializer_{}", target.0);
        let previous = self.scopes[module]
            .values
            .insert(name.clone(), Symbol::Function(target));
        let call = ast::Expr {
            kind: ast::ExprKind::Call {
                callee: Box::new(ast::Expr {
                    kind: ast::ExprKind::Name(vec![ast::Text::new(&name, span)]),
                    span,
                }),
                args: args.to_vec(),
            },
            span,
        };
        let checked = self.expr(module, &call, locals, None);
        match previous {
            Some(symbol) => {
                self.scopes[module].values.insert(name, symbol);
            }
            None => {
                self.scopes[module].values.remove(&name);
            }
        }
        checked
    }
    fn with_expression_effects(
        &mut self,
        module: usize,
        value: &ir::Expr,
    ) -> Result<Vec<TypeId>, Diagnostic> {
        let functions = self.checked.iter().flatten().cloned().collect::<Vec<_>>();
        super::super::effects::incoming(
            value,
            &functions,
            &self.traits.registry,
            &mut self.catalog,
            &mut self.solver,
            &self.schemes.parameters,
            &self.modules.modules[module].source_name,
        )
    }
    fn with_effect_members(
        &mut self,
        module: usize,
        values: &[TypeId],
        span: Span,
        result: &mut HashSet<EffectId>,
    ) -> Result<(), Diagnostic> {
        let mut pending = values.to_vec();
        let mut seen = HashSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            let ty = self.resolve_type(module, data::frontend_type(ty), span)?;
            match self.catalog.core.ty(data::backend_type(ty)) {
                Type::Handler(effect) => {
                    result.insert(*effect);
                }
                Type::Set(summary) => pending.extend(&summary.params),
                _ => {}
            }
        }
        Ok(())
    }
    fn with_handler_requirements(
        &mut self,
        module: usize,
        id: usize,
        span: Span,
    ) -> Result<Vec<EffectId>, Diagnostic> {
        let handler = self.effects.handlers[id].clone();
        let mut required = HashSet::new();
        for target in handler
            .init
            .into_iter()
            .chain(handler.release)
            .chain(handler.operations)
        {
            self.function(target)?;
            let effects = self.definitions[target.0].signature.sets.effects.clone();
            self.with_effect_members(module, &effects, span, &mut required)?;
        }
        let mut required = required.into_iter().collect::<Vec<_>>();
        required.sort_by_key(|id| id.into_raw());
        Ok(required)
    }
}
fn with_reference(local: &Local, span: Span) -> ir::Expr {
    ir::Expr {
        kind: ir::ExprKind::Local(local.id),
        ty: local.ty,
        span,
    }
}

#[path = "with/closures.rs"]
mod closures;
