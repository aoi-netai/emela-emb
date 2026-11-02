//! Product updates retain the base's unchanged fields, with fresh result type arguments.
use super::*;
use crate::contract::ir::{CtorRef, Type};

impl Checker {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn record_update(
        &mut self,
        module: usize,
        callee: &ast::Expr,
        updates: &[ast::Argument],
        base: &ast::Expr,
        locals: &mut Locals,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<ir::Expr, Diagnostic> {
        let invalid = || {
            self.error(
                module,
                DK::Type,
                callee.span,
                "record update requires a type, error, or handler constructor with fields",
            )
        };
        let ast::ExprKind::Name(path) = &callee.kind else {
            return Err(invalid());
        };
        if path.len() == 1 && locals.names.contains_key(&path[0].value) {
            return Err(invalid());
        }
        // Resolve first, so an unknown name retains its resolution diagnostic.
        let Symbol::Constructor(ctor) = self.resolve(module, path)? else {
            return Err(invalid());
        };
        let declaration = self.catalog.declaration(ctor.enum_id).clone();
        if !declaration.ast.record {
            return Err(invalid());
        }
        if self.effects.handlers.iter().any(|handler| {
            matches!(self.catalog.core.ty(handler.state_type), Type::Enum(id, _) if *id == ctor.enum_id)
                && !handler.fields_parenthesized
        }) {
            return Err(invalid());
        }
        self.check_handler_construction(module, ctor, span)?;
        let fields = &declaration.ast.variants[ctor.variant].fields;
        let raw_types = self.catalog.core.enums[ctor.enum_id].variants[ctor.variant]
            .tys
            .clone();
        let mut written = vec![None; fields.len()];
        for (index, argument) in updates.iter().enumerate() {
            let Some(label) = &argument.label else {
                return Err(self.error(
                    module,
                    DK::Type,
                    argument.value.span,
                    "record update fields must be named",
                ));
            };
            let field = fields
                .iter()
                .position(|field| field.name.as_ref().is_some_and(|n| n.value == label.value))
                .ok_or_else(|| {
                    self.error(
                        module,
                        DK::Name,
                        label.span,
                        format!("unknown record update field '{}'", label.value),
                    )
                })?;
            if written[field].replace(index).is_some() {
                return Err(self.error(
                    module,
                    DK::Type,
                    label.span,
                    "record update field is supplied twice",
                ));
            }
        }
        let mut bindings = vec![None; declaration.ast.params.len()];
        if let Some(expected) = expected {
            let expected = self.resolve_type(module, expected, span)?;
            if let Type::Enum(id, arguments) = self.catalog.core.ty(data::backend_type(expected)) {
                if *id == ctor.enum_id {
                    bindings = arguments.iter().copied().map(Some).collect();
                }
            }
        }
        // Inspect the base type without unifying its parameters with the result's parameters.
        // The runtime statements below still evaluate the written updates before this base.
        let mut base_value = self.expr(module, base, locals, None)?;
        base_value.ty = self.resolve_type(module, base_value.ty, base.span)?;
        let source_arguments = match self.catalog.core.ty(data::backend_type(base_value.ty)) {
            Type::Enum(id, arguments) if *id == ctor.enum_id => arguments.clone(),
            Type::Never => self.constructor_arguments(module, &mut bindings, span)?,
            _ => {
                return Err(self.error(
                    module,
                    DK::Type,
                    base.span,
                    format!(
                        "record update base must have type '{}'",
                        declaration.ast.name.value
                    ),
                ));
            }
        };
        let mut inherited_types = Vec::with_capacity(fields.len());
        for (index, &template) in raw_types.iter().enumerate() {
            let actual = self.catalog.effects.substitute_type(
                &mut self.catalog.core,
                template,
                &source_arguments,
            );
            inherited_types.push(data::frontend_type(actual));
            if written[index].is_none() {
                self.infer_constructor(module, template, actual, &mut bindings, base.span)?;
            }
        }
        let mut statements = Vec::new();
        let mut updated_values = Vec::new();
        for (index, argument) in updates.iter().enumerate() {
            let field = written
                .iter()
                .position(|entry| *entry == Some(index))
                .unwrap();
            let arguments = self.constructor_arguments(module, &mut bindings, span)?;
            let field_type = self.catalog.effects.substitute_type(
                &mut self.catalog.core,
                raw_types[field],
                &arguments,
            );
            let value = self.expr(
                module,
                &argument.value,
                locals,
                Some(data::frontend_type(field_type)),
            )?;
            self.infer_constructor(
                module,
                raw_types[field],
                data::backend_type(value.ty),
                &mut bindings,
                argument.value.span,
            )?;
            updated_values.push(record_local(locals, value, &mut statements));
        }
        let result_arguments = self.constructor_arguments(module, &mut bindings, span)?;
        let value_type = Ty::Compound(
            self.catalog
                .core
                .types
                .enum_type(ctor.enum_id, result_arguments.clone()),
        );
        if let Some(expected) = expected {
            self.expect_type(module, value_type, expected, span)?;
        }
        let value_type = self.resolve_type(module, value_type, span)?;
        self.validate_type_bounds(
            module,
            data::backend_type(value_type),
            &self.current_bounds.clone(),
            span,
        )?;
        // Never is permitted at the top level of an expected position, as in an ordinary call.
        if base_value.ty == Ty::Never {
            base_value.ty = value_type;
        }
        let base_local = record_local(locals, base_value, &mut statements);
        let mut args = Vec::new();
        for (index, written) in written.iter().enumerate() {
            if let Some(written) = written {
                args.push(updated_values[*written].clone());
            } else {
                let field_type = self.catalog.effects.substitute_type(
                    &mut self.catalog.core,
                    raw_types[index],
                    &result_arguments,
                );
                let field = ir::Expr {
                    kind: ir::ExprKind::Field {
                        base: Box::new(base_local.clone()),
                        ctor,
                        index,
                    },
                    ty: inherited_types[index],
                    span: base.span,
                };
                args.push(self.coerce_function(
                    module,
                    field,
                    Some(data::frontend_type(field_type)),
                    base.span,
                )?);
            }
        }
        statements.push(ir::Statement::Expr(ir::Expr {
            kind: ir::ExprKind::Make {
                ctor,
                order: (0..args.len()).collect(),
                args,
            },
            ty: value_type,
            span,
        }));
        Ok(ir::Expr {
            kind: ir::ExprKind::Block(ir::Block {
                statements,
                ty: value_type,
                span,
            }),
            ty: value_type,
            span,
        })
    }

    pub(super) fn check_handler_construction(
        &mut self,
        module: usize,
        ctor: CtorRef,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if self.catalog.declaration(ctor.enum_id).ast.params.is_empty() {
            let state = self.catalog.core.types.enum_type(ctor.enum_id, vec![]);
            if let Some(handler) = self.effects.handler_for_state(state) {
                if !matches!(self.current_effect_method, Some(super::super::effect_collect::MethodKind::Init(id)) if id == handler.id)
                {
                    return Err(self.error(
                        module,
                        DK::Type,
                        span,
                        "a handler state constructor is available only inside its initializer",
                    ));
                }
            }
        }
        Ok(())
    }
}

fn record_local(
    locals: &mut Locals,
    value: ir::Expr,
    statements: &mut Vec<ir::Statement>,
) -> ir::Expr {
    let ty = value.ty;
    let span = value.span;
    let local = Local {
        id: LocalId(locals.next),
        name: "record update".into(),
        ty,
    };
    locals.next += 1;
    statements.push(ir::Statement::Pattern {
        pattern: ir::Pattern {
            kind: ir::PatternKind::Bind(local.clone()),
            ty,
            span,
        },
        value,
    });
    ir::Expr {
        kind: ir::ExprKind::Local(local.id),
        ty,
        span,
    }
}
