//! Resolve dot calls from the receiver's head and the caller's visible traits.
use super::*;
use crate::contract::ir::{Type, TypeId};
use crate::frontend::traits::{Registry, TraitId};

#[derive(Clone, Copy)]
enum Candidate {
    Function(DefinitionId),
    Trait(TraitId),
}
impl Checker {
    pub(super) fn validate_method_declarations(&self) -> Result<(), Diagnostic> {
        for definition in &self.definitions {
            let Some(&parameter) = definition.signature.params.first() else {
                continue;
            };
            let Type::Enum(id, _) = self.catalog.core.ty(data::backend_type(parameter)) else {
                continue;
            };
            let declaration = self.catalog.declaration(*id);
            if declaration.module == definition.module
                && declaration.ast.variants.len() == 1
                && declaration.ast.variants[0].fields.iter().any(|field| {
                    field
                        .name
                        .as_ref()
                        .is_some_and(|field| field.value == definition.ast.name.value)
                })
            {
                return Err(self.error(
                    definition.module,
                    DK::Name,
                    definition.ast.name.span,
                    "a receiver function and a record field cannot have the same name",
                ));
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn method_call(
        &mut self,
        module: usize,
        base: &ast::Expr,
        name: &ast::Text,
        args: &[ast::Argument],
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let mut receiver = self.expr(module, base, locals, None)?;
        receiver.ty = self.resolve_type(module, receiver.ty, base.span)?;
        let receiver_type = data::backend_type(receiver.ty);
        if self.solver.is_meta(receiver_type) {
            return Err(self.error(
                module,
                DK::Type,
                base.span,
                "the receiver type must be determined before a method call",
            ));
        }
        let mut candidates = Vec::new();
        if let Some(owner) = self.receiver_module(receiver_type) {
            if let Type::Enum(id, _) = self.catalog.core.ty(receiver_type) {
                let declaration = self.catalog.declaration(*id);
                if declaration.ast.variants.len() == 1
                    && declaration.ast.variants[0].fields.iter().any(|field| {
                        field
                            .name
                            .as_ref()
                            .is_some_and(|field| field.value == name.value)
                    })
                    && (!declaration.ast.opaque || owner == module)
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        name.span,
                        "calling a record field requires a function value",
                    ));
                }
            }
            if let Some(&id) = self.module_definitions[owner].get(&name.value) {
                let definition = &self.definitions[id.0];
                if (owner == module || definition.ast.public)
                    && !definition.ast.constant
                    && definition
                        .signature
                        .params
                        .first()
                        .is_some_and(|&parameter| {
                            Registry::head_of(&self.catalog.core, data::backend_type(parameter))
                                .ok()
                                .flatten()
                                == Registry::head_of(&self.catalog.core, receiver_type)
                                    .ok()
                                    .flatten()
                        })
                {
                    candidates.push(Candidate::Function(id));
                }
            }
        }
        let traits = if matches!(self.catalog.core.ty(receiver_type), Type::Param(_)) {
            self.current_bounds
                .get(&receiver_type)
                .cloned()
                .unwrap_or_default()
        } else {
            self.traits.visible_traits(module)
        };
        let mut owners = HashSet::new();
        for trait_id in traits {
            if let Ok((owner, _, names)) =
                self.trait_method(module, trait_id, &name.value, name.span)
            {
                if names.first().is_some_and(|name| name == "self")
                    && self
                        .traits
                        .registry
                        .satisfies(
                            &self.catalog.core,
                            receiver_type,
                            owner,
                            &self.current_bounds,
                        )
                        .map_err(|e| self.error(module, DK::Type, span, e.message))?
                    && owners.insert(owner)
                {
                    candidates.push(Candidate::Trait(owner));
                }
            }
        }
        let candidate = match candidates.as_slice() {
            [candidate] => *candidate,
            [] => {
                return Err(self.error(
                    module,
                    DK::Name,
                    name.span,
                    format!(
                        "no accessible method '{}' for this receiver type",
                        name.value
                    ),
                ));
            }
            _ => {
                return Err(self.error(
                    module,
                    DK::Name,
                    name.span,
                    format!("method '{}' is ambiguous", name.value),
                ));
            }
        };
        // The receiver is checked and evaluated once, before the arguments. A private
        // compiler binding lets the ordinary call checker retain all named-argument rules.
        let alias = format!("$receiver_{}", locals.next);
        let local = locals.bind(alias.clone(), receiver.ty);
        let mut arguments = vec![ast::Argument {
            label: None,
            value: ast::Expr {
                kind: ast::ExprKind::Name(vec![ast::Text::new(&alias, base.span)]),
                span: base.span,
            },
        }];
        arguments.extend_from_slice(args);
        let result = match candidate {
            Candidate::Trait(trait_id) => self.source_trait_call(
                module,
                trait_id,
                &name.value,
                &arguments,
                locals,
                expected,
                span,
            ),
            Candidate::Function(id) => {
                let function_alias = format!("$method_{}_{}", id.0, local.id.0);
                let previous = self.scopes[module]
                    .values
                    .insert(function_alias.clone(), Symbol::Function(id));
                let result = self.expr(
                    module,
                    &ast::Expr {
                        span,
                        kind: ast::ExprKind::Call {
                            callee: Box::new(ast::Expr {
                                span: name.span,
                                kind: ast::ExprKind::Name(vec![ast::Text::new(
                                    &function_alias,
                                    name.span,
                                )]),
                            }),
                            args: arguments,
                        },
                    },
                    locals,
                    expected,
                );
                if let Some(previous) = previous {
                    self.scopes[module].values.insert(function_alias, previous);
                } else {
                    self.scopes[module].values.remove(&function_alias);
                }
                result
            }
        };
        locals.names.remove(&alias);
        let call = result?;
        Ok(ir::Expr {
            ty: call.ty,
            span,
            kind: ir::ExprKind::Block(ir::Block {
                ty: call.ty,
                span,
                statements: vec![
                    ir::Statement::Pattern {
                        pattern: ir::Pattern {
                            kind: ir::PatternKind::Bind(local),
                            ty: receiver.ty,
                            span: base.span,
                        },
                        value: receiver,
                    },
                    ir::Statement::Expr(call),
                ],
            }),
        })
    }
    fn receiver_module(&self, ty: TypeId) -> Option<usize> {
        let name = match self.catalog.core.ty(ty) {
            Type::Enum(id, _) => return Some(self.catalog.declaration(*id).module),
            Type::Int => "Int",
            Type::Int64 => "Int64",
            Type::Float => "Float",
            Type::Bool => "Bool",
            Type::String => "String",
            Type::Unit => "Unit",
            _ => return None,
        };
        self.modules
            .modules
            .iter()
            .position(|module| module.builtin && module.name == name)
    }
}
