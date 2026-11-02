//! Check capability values and the provenance retained by escaping closures.
use super::{data, ir, types::ValueType};
use crate::{Diagnostic, DiagnosticKind, Span, contract::ir as b};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Effect {
    declaration: String,
    arguments: Vec<b::TypeId>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Owner {
    Function(usize),
    Lambda { function: usize, site: usize },
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Parameter(Owner, usize),
    Capture(Owner),
    Effect(Effect),
    Operation(Effect, usize),
    Provider(Owner, usize),
    Invocation(Box<Origin>),
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Callable {
    owner: Owner,
    arguments: Vec<b::TypeId>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Flow {
    origins: BTreeSet<Origin>,
    callables: BTreeSet<Callable>,
    operations: BTreeMap<usize, Flow>,
    /// Handler operations use the evidence at construction.
    evidence: BTreeMap<Effect, BTreeSet<Origin>>,
}
impl Flow {
    fn origin(origin: Origin) -> Self {
        Self {
            origins: BTreeSet::from([origin]),
            ..Self::default()
        }
    }
    fn merge(&mut self, other: Self) {
        self.origins.extend(other.origins);
        self.callables.extend(other.callables);
        for (op, flow) in other.operations {
            self.operations.entry(op).or_default().merge(flow);
        }
        for (effect, origins) in other.evidence {
            self.evidence.entry(effect).or_default().extend(origins);
        }
    }
}
type Locals = HashMap<ir::LocalId, Flow>;
type Providers = BTreeMap<Effect, Flow>;
type Summaries = BTreeMap<Owner, Flow>;

pub(crate) fn audit(program: &ir::Program) -> Result<(), Vec<Diagnostic>> {
    let mut core = program.core.clone();
    let mut summaries = Summaries::new();
    for round in 0..128 {
        let mut next = summaries.clone();
        let mut errors = Vec::new();
        for function in &program.functions {
            let mut context = Context {
                core: &mut core,
                summaries: &summaries,
                next: &mut next,
                function,
                owner: Owner::Function(function.id.0),
                site: 0,
                errors: &mut errors,
            };
            let mut locals = HashMap::new();
            for (index, parameter) in function.params.iter().enumerate() {
                let flow = context.parameter(parameter.ty, context.owner, index);
                locals.insert(parameter.id, flow);
            }
            let result = match &function.body {
                Some(body) => {
                    let result = context.block(body, &mut locals, &Providers::new());
                    if let Some(value) = tail(body) {
                        context.reject_capability(value, "a capability value cannot be returned");
                    }
                    result
                }
                None => {
                    let mut result = Flow::default();
                    if function
                        .signature
                        .ret
                        .is_some_and(|ty| context.can_retain(ty))
                    {
                        for flow in locals.into_values() {
                            result.merge(flow);
                        }
                    }
                    result
                }
            };
            context.next.insert(context.owner, result);
        }
        if next == summaries {
            errors.sort_by(|a, b| {
                (&a.source, a.span.start, a.span.end, &a.message).cmp(&(
                    &b.source,
                    b.span.start,
                    b.span.end,
                    &b.message,
                ))
            });
            errors.dedup();
            return if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            };
        }
        summaries = next;
        if round == 127 {
            return Err(vec![Diagnostic::new(
                DiagnosticKind::Type,
                Span::new(0, 0),
                "capability capture inference does not converge",
            )]);
        }
    }
    unreachable!()
}

struct Context<'a> {
    core: &'a mut b::Definitions,
    summaries: &'a Summaries,
    next: &'a mut Summaries,
    function: &'a ir::Function,
    owner: Owner,
    site: usize,
    errors: &'a mut Vec<Diagnostic>,
}
impl Context<'_> {
    fn error(&mut self, span: Span, message: &str) {
        self.errors.push(
            Diagnostic::new(DiagnosticKind::Type, span, message)
                .in_source(&self.function.source_name),
        );
    }
    fn reject_capability(&mut self, value: &ir::Expr, message: &str) {
        if matches!(
            self.core.ty(data::backend_type(value.ty)),
            b::Type::Handler(_)
        ) {
            self.error(value.span, message);
        }
    }
    fn effect(&self, id: b::EffectId) -> Effect {
        Effect {
            declaration: self.core.effects[id].decl.clone(),
            arguments: self.core.effects[id].args.clone(),
        }
    }
    fn parameter(&self, ty: ValueType, owner: Owner, index: usize) -> Flow {
        if self.can_retain(ty) {
            Flow::origin(Origin::Parameter(owner, index))
        } else {
            Flow::default()
        }
    }
    fn can_retain(&self, ty: ValueType) -> bool {
        fn visit(core: &b::Definitions, ty: b::TypeId, seen: &mut HashSet<b::TypeId>) -> bool {
            if !seen.insert(ty) {
                return false;
            }
            match core.ty(ty) {
                b::Type::Handler(_) | b::Type::Fn(_) | b::Type::Param(_) | b::Type::Any => true,
                b::Type::Tuple(fields) => fields.iter().any(|&field| visit(core, field, seen)),
                b::Type::List(item) | b::Type::Cell(item) => visit(core, *item, seen),
                b::Type::Enum(id, args) => {
                    args.iter().any(|&arg| visit(core, arg, seen))
                        || core.enums[*id]
                            .variants
                            .iter()
                            .flat_map(|variant| &variant.tys)
                            .any(|&field| visit(core, field, seen))
                }
                _ => false,
            }
        }
        visit(self.core, data::backend_type(ty), &mut HashSet::new())
    }
    fn block(&mut self, block: &ir::Block, locals: &mut Locals, providers: &Providers) -> Flow {
        let mut result = Flow::default();
        for statement in &block.statements {
            match statement {
                ir::Statement::Pattern { pattern, value } => {
                    let flow = self.expression(value, locals, providers);
                    bind(pattern, &flow, locals);
                    result = Flow::default();
                }
                ir::Statement::Expr(value) => result = self.expression(value, locals, providers),
            }
        }
        result
    }
    fn arguments(
        &mut self,
        arguments: &[ir::Expr],
        locals: &mut Locals,
        providers: &Providers,
        data: bool,
    ) -> Vec<Flow> {
        arguments
            .iter()
            .map(|argument| {
                self.reject_capability(
                    argument,
                    if data {
                        "a capability value cannot be stored in data"
                    } else {
                        "a capability value cannot be passed as an argument"
                    },
                );
                self.expression(argument, locals, providers)
            })
            .collect()
    }
    fn expression(&mut self, value: &ir::Expr, locals: &mut Locals, providers: &Providers) -> Flow {
        use ir::ExprKind as E;
        let site = self.site;
        self.site += 1;
        let mut result = match &value.kind {
            E::Use { effect } => {
                let effect = self.effect(*effect);
                providers
                    .get(&effect)
                    .cloned()
                    .unwrap_or_else(|| Flow::origin(Origin::Effect(effect)))
            }
            E::Local(id) => locals.get(id).cloned().unwrap_or_default(),
            E::FnRef {
                target,
                type_arguments,
            } => Flow {
                callables: BTreeSet::from([Callable {
                    owner: Owner::Function(target.0),
                    arguments: type_arguments.clone(),
                }]),
                ..Flow::default()
            },
            E::Lambda {
                params,
                captures,
                body,
            } => {
                let owner = Owner::Lambda {
                    function: self.function.id.0,
                    site,
                };
                let mut result = Flow::default();
                let mut lambda_locals = Locals::new();
                for capture in captures {
                    let mut flow = locals.get(capture).cloned().unwrap_or_default();
                    result.origins.extend(flow.origins.iter().cloned());
                    if !flow.origins.is_empty()
                        || !flow.callables.is_empty()
                        || !flow.operations.is_empty()
                    {
                        flow.origins = BTreeSet::from([Origin::Capture(owner)]);
                    }
                    lambda_locals.insert(*capture, flow);
                }
                for (index, parameter) in params.iter().enumerate() {
                    lambda_locals.insert(parameter.id, self.parameter(parameter.ty, owner, index));
                }
                let saved = self.owner;
                self.owner = owner;
                let returned = self.block(body, &mut lambda_locals, &Providers::new());
                if let Some(value) = tail(body) {
                    self.reject_capability(
                        value,
                        "a capability value cannot be returned from an anonymous function",
                    );
                }
                self.next.insert(owner, returned);
                self.owner = saved;
                let arguments = (0..self.function.type_parameters)
                    .map(|index| self.core.types.intern(b::Type::Param(index)))
                    .collect();
                result.callables.insert(Callable { owner, arguments });
                result
            }
            E::Call {
                target,
                type_arguments,
                args,
                order,
            } => {
                let arguments = self.arguments(args, locals, providers, false);
                let callable = Flow {
                    callables: BTreeSet::from([Callable {
                        owner: Owner::Function(target.0),
                        arguments: type_arguments.clone(),
                    }]),
                    ..Flow::default()
                };
                self.invoke(&callable, &arguments, order, providers, 0)
            }
            E::Apply {
                callee,
                args,
                order,
            } => {
                let callee = self.expression(callee, locals, providers);
                let arguments = self.arguments(args, locals, providers, false);
                self.invoke(&callee, &arguments, order, providers, 0)
            }
            E::OpCall {
                handler,
                effect,
                op,
                args,
                order,
                ..
            } => {
                let handler = self.expression(handler, locals, providers);
                let arguments = self.arguments(args, locals, providers, false);
                if let Some(operation) = handler.operations.get(op) {
                    self.invoke(operation, &arguments, order, providers, 0)
                } else {
                    let mut result = Flow::origin(Origin::Operation(self.effect(*effect), *op));
                    for argument in arguments {
                        result.merge(argument);
                    }
                    result
                }
            }
            E::Handler { operations, .. } => {
                let mut result = Flow::default();
                for (index, operation) in operations.iter().enumerate() {
                    let mut flow = self.expression(operation, locals, providers);
                    flow.evidence = providers
                        .iter()
                        .map(|(effect, provider)| (effect.clone(), provider.origins.clone()))
                        .collect();
                    result.origins.extend(flow.origins.iter().cloned());
                    result.operations.insert(index, flow);
                }
                result
            }
            E::Provide { bindings, body } => {
                let mut provided = providers.clone();
                for (effect, binding) in bindings {
                    let flow = self.expression(binding, locals, providers);
                    provided.insert(self.effect(*effect), flow);
                }
                self.block(body, &mut locals.clone(), &provided)
            }
            E::With { bindings, body } => {
                let mut provided = providers.clone();
                let mut inner = locals.clone();
                let mut introduced = BTreeSet::new();
                for binding in bindings {
                    let mut flow = self.expression(&binding.value, &mut inner, &provided);
                    if binding.retire.is_some() {
                        let origin = Origin::Provider(self.owner, binding.local.id.0);
                        introduced.insert(origin.clone());
                        flow.origins.insert(origin);
                    }
                    inner.insert(binding.local.id, flow.clone());
                    for effect in &binding.effects {
                        provided.insert(self.effect(*effect), flow.clone());
                    }
                }
                let result = self.block(body, &mut inner, &provided);
                if result
                    .origins
                    .iter()
                    .any(|origin| retains_provider(origin, &introduced))
                {
                    self.error(
                        value.span,
                        "a capability or a closure retaining it cannot escape its with scope",
                    );
                }
                for binding in bindings.iter().rev() {
                    if let Some(cleanup) = &binding.cleanup {
                        self.expression(cleanup, &mut inner, &provided);
                    }
                }
                result
            }
            E::Tuple(arguments)
            | E::Make {
                args: arguments, ..
            } => {
                let mut result = Flow::default();
                for argument in self.arguments(arguments, locals, providers, true) {
                    result.merge(argument);
                }
                result
            }
            E::List { elements, tail } => {
                let mut result = Flow::default();
                for element in self.arguments(elements, locals, providers, true) {
                    result.merge(element);
                }
                if let Some(tail) = tail {
                    result.merge(self.expression(tail, locals, providers));
                }
                result
            }
            E::TraitCall { args, .. } | E::Builtin { args, .. } => {
                let mut result = Flow::default();
                for argument in self.arguments(args, locals, providers, false) {
                    result.merge(argument);
                }
                result
            }
            E::Match { value, arms } | E::Escape { value, arms, .. } => {
                let matched = self.expression(value, locals, providers);
                let mut result = Flow::default();
                for arm in arms {
                    let mut inner = locals.clone();
                    bind(&arm.pattern, &matched, &mut inner);
                    if let Some(guard) = &arm.guard {
                        self.expression(guard, &mut inner, providers);
                    }
                    result.merge(self.expression(&arm.value, &mut inner, providers));
                }
                result
            }
            E::If {
                condition,
                then,
                else_,
            } => {
                self.expression(condition, locals, providers);
                let mut result = self.block(then, &mut locals.clone(), providers);
                result.merge(self.block(else_, &mut locals.clone(), providers));
                result
            }
            E::Block(block) => self.block(block, &mut locals.clone(), providers),
            E::Field { base, .. } | E::Coerce(base) | E::Tag(base) => {
                self.expression(base, locals, providers)
            }
            E::Unary { value, .. } | E::Assert(value) | E::Fail(value) => {
                self.expression(value, locals, providers);
                Flow::default()
            }
            E::Binary { left, right, .. } => {
                self.expression(left, locals, providers);
                self.expression(right, locals, providers);
                Flow::default()
            }
            E::Concat(values) => {
                for value in values {
                    self.expression(value, locals, providers);
                }
                Flow::default()
            }
            E::Literal(_) => Flow::default(),
        };
        if !self.can_retain(value.ty) {
            result = Flow::default();
        }
        result
    }

    fn instantiate_effect(&mut self, effect: &Effect, arguments: &[b::TypeId]) -> Effect {
        Effect {
            declaration: effect.declaration.clone(),
            arguments: effect
                .arguments
                .iter()
                .map(|&argument| data::substitute(&mut self.core.types, argument, arguments))
                .collect(),
        }
    }
    fn invoke(
        &mut self,
        callee: &Flow,
        args: &[Flow],
        order: &[usize],
        providers: &Providers,
        depth: usize,
    ) -> Flow {
        if callee.callables.is_empty() || depth >= 64 {
            let mut result = Flow::default();
            for origin in &callee.origins {
                result
                    .origins
                    .insert(Origin::Invocation(Box::new(origin.clone())));
            }
            for argument in args {
                result.origins.extend(argument.origins.iter().cloned());
            }
            return result;
        }
        let mut result = Flow::default();
        for callable in &callee.callables {
            let summary = self
                .summaries
                .get(&callable.owner)
                .cloned()
                .unwrap_or_default();
            let mut returned = Flow::default();
            for origin in &summary.origins {
                returned.origins.extend(self.resolve_origin(
                    origin,
                    callable,
                    callee,
                    args,
                    order,
                    providers,
                    depth + 1,
                ));
            }
            for nested in summary.callables {
                returned.callables.insert(Callable {
                    owner: nested.owner,
                    arguments: nested
                        .arguments
                        .into_iter()
                        .map(|ty| data::substitute(&mut self.core.types, ty, &callable.arguments))
                        .collect(),
                });
            }
            result.merge(returned);
        }
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn resolve_origin(
        &mut self,
        origin: &Origin,
        callable: &Callable,
        callee: &Flow,
        args: &[Flow],
        order: &[usize],
        providers: &Providers,
        depth: usize,
    ) -> BTreeSet<Origin> {
        match origin {
            Origin::Parameter(owner, parameter) if *owner == callable.owner => order
                .get(*parameter)
                .and_then(|&index| args.get(index))
                .map(|arg| arg.origins.clone())
                .unwrap_or_default(),
            Origin::Capture(owner) if *owner == callable.owner => callee.origins.clone(),
            Origin::Effect(effect) => {
                let effect = self.instantiate_effect(effect, &callable.arguments);
                callee
                    .evidence
                    .get(&effect)
                    .cloned()
                    .or_else(|| {
                        providers
                            .get(&effect)
                            .map(|provider| provider.origins.clone())
                    })
                    .unwrap_or_else(|| BTreeSet::from([Origin::Effect(effect)]))
            }
            Origin::Operation(effect, op) => {
                let effect = self.instantiate_effect(effect, &callable.arguments);
                if let Some(operation) = providers
                    .get(&effect)
                    .and_then(|provider| provider.operations.get(op))
                    .cloned()
                {
                    self.invoke(&operation, &[], &[], providers, depth).origins
                } else {
                    BTreeSet::from([Origin::Operation(effect, *op)])
                }
            }
            Origin::Invocation(inner) => {
                let argument = match inner.as_ref() {
                    Origin::Parameter(owner, parameter) if *owner == callable.owner => order
                        .get(*parameter)
                        .and_then(|&index| args.get(index))
                        .cloned(),
                    Origin::Capture(owner) if *owner == callable.owner => Some(callee.clone()),
                    _ => None,
                };
                if let Some(argument) = argument {
                    self.invoke(&argument, &[], &[], providers, depth).origins
                } else {
                    self.resolve_origin(inner, callable, callee, args, order, providers, depth)
                        .into_iter()
                        .map(|origin| match origin {
                            Origin::Provider(..) => origin,
                            _ => Origin::Invocation(Box::new(origin)),
                        })
                        .collect()
                }
            }
            _ => BTreeSet::from([origin.clone()]),
        }
    }
}

fn tail(block: &ir::Block) -> Option<&ir::Expr> {
    match block.statements.last()? {
        ir::Statement::Expr(value) => Some(value),
        ir::Statement::Pattern { .. } => None,
    }
}
fn retains_provider(origin: &Origin, providers: &BTreeSet<Origin>) -> bool {
    providers.contains(origin)
        || matches!(origin, Origin::Invocation(inner) if retains_provider(inner, providers))
}
fn bind(pattern: &ir::Pattern, flow: &Flow, locals: &mut Locals) {
    match &pattern.kind {
        ir::PatternKind::Bind(local) => {
            locals.insert(local.id, flow.clone());
        }
        ir::PatternKind::Tuple(fields) | ir::PatternKind::Constructor { fields, .. } => {
            for field in fields {
                bind(field, flow, locals);
            }
        }
        ir::PatternKind::ListCons { head, tail } => {
            bind(head, flow, locals);
            bind(tail, flow, locals);
        }
        ir::PatternKind::Constant { .. }
        | ir::PatternKind::ListNil
        | ir::PatternKind::Any
        | ir::PatternKind::Literal(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn span() -> Span {
        Span::new(0, 1)
    }
    fn expression(kind: ir::ExprKind, ty: ValueType) -> ir::Expr {
        ir::Expr {
            kind,
            ty,
            span: span(),
        }
    }
    fn block(value: ir::Expr) -> ir::Block {
        ir::Block {
            ty: value.ty,
            statements: vec![ir::Statement::Expr(value)],
            span: span(),
        }
    }
    fn integer() -> ir::Expr {
        expression(ir::ExprKind::Literal(ir::Literal::Int(42)), ValueType::Int)
    }
    fn reference(local: &ir::Local) -> ir::Expr {
        expression(ir::ExprKind::Local(local.id), local.ty)
    }
    fn function(id: usize, params: Vec<ir::Local>, body: ir::Expr) -> ir::Function {
        ir::Function {
            id: ir::DefinitionId(id),
            type_parameters: 0,
            constant: false,
            origin: None,
            derivation: None,
            name: format!("function_{id}"),
            source_name: "main.emel".into(),
            span: span(),
            exported: false,
            export_symbol: None,
            signature: super::super::types::Signature {
                params: params.iter().map(|param| param.ty).collect(),
                ret: Some(body.ty),
                sets: Default::default(),
            },
            params,
            external: None,
            intrinsic: None,
            body: Some(block(body)),
        }
    }
    struct Fixture {
        core: b::Definitions,
        effect: b::EffectId,
        cap: ir::Local,
        callback: ValueType,
    }
    impl Fixture {
        fn new() -> Self {
            let mut core = b::Definitions::default();
            let effect = core.effects.alloc(b::EffectDef {
                name: "Read".into(),
                decl: "Read".into(),
                args: vec![],
                label_of: None,
                ops: vec![b::OpDef {
                    name: "read".into(),
                    suspend: false,
                    type_params: 0,
                    params: vec![],
                    ret: b::TypeId::INT,
                    fails: false,
                }],
            });
            let cap = ir::Local {
                id: ir::LocalId(0),
                name: "cap".into(),
                ty: data::frontend_type(core.types.intern(b::Type::Handler(effect))),
            };
            let callback = Self::fn_type(&mut core, b::TypeId::INT);
            Self {
                core,
                effect,
                cap,
                callback,
            }
        }
        fn fn_type(core: &mut b::Definitions, ret: b::TypeId) -> ValueType {
            data::frontend_type(core.types.intern(b::Type::Fn(b::FnType {
                params: vec![],
                ret,
                fails: false,
                suspends: false,
                ev: false,
                sets: None,
            })))
        }
        fn closure(&self, capture: bool) -> ir::Expr {
            let body = if capture {
                expression(
                    ir::ExprKind::OpCall {
                        handler: Box::new(reference(&self.cap)),
                        effect: self.effect,
                        op: 0,
                        errors: vec![],
                        args: vec![],
                        order: vec![],
                    },
                    ValueType::Int,
                )
            } else {
                integer()
            };
            self.lambda(body, self.callback, capture)
        }
        fn lambda(&self, body: ir::Expr, ty: ValueType, capture: bool) -> ir::Expr {
            expression(
                ir::ExprKind::Lambda {
                    params: vec![],
                    captures: if capture { vec![self.cap.id] } else { vec![] },
                    body: block(body),
                },
                ty,
            )
        }
        fn scope(&self, value: ir::Expr, retire: bool) -> ir::Expr {
            let table = ir::Local {
                id: ir::LocalId(10),
                name: "table".into(),
                ty: self.cap.ty,
            };
            let body = ir::Block {
                ty: value.ty,
                span: span(),
                statements: vec![
                    ir::Statement::Pattern {
                        pattern: ir::Pattern {
                            kind: ir::PatternKind::Bind(self.cap.clone()),
                            ty: self.cap.ty,
                            span: span(),
                        },
                        value: expression(
                            ir::ExprKind::Use {
                                effect: self.effect,
                            },
                            self.cap.ty,
                        ),
                    },
                    ir::Statement::Expr(value),
                ],
            };
            let ty = body.ty;
            expression(
                ir::ExprKind::With {
                    bindings: vec![ir::WithBinding {
                        local: table,
                        value: expression(
                            ir::ExprKind::Handler {
                                name: "One".into(),
                                effect: self.effect,
                                operations: vec![self.closure(false)],
                            },
                            self.cap.ty,
                        ),
                        effects: vec![self.effect],
                        cleanup: None,
                        retire: retire.then(|| "One".into()),
                    }],
                    body,
                },
                ty,
            )
        }
        fn audit(self, bodies: Vec<ir::Function>) -> Result<(), Vec<Diagnostic>> {
            audit(&ir::Program {
                core: self.core,
                registry: Default::default(),
                functions: bodies,
            })
        }
    }

    #[test]
    fn direct_capability_arguments_data_and_returns_are_rejected() {
        for kind in 0..3 {
            let f = Fixture::new();
            let cap = expression(ir::ExprKind::Use { effect: f.effect }, f.cap.ty);
            let value = match kind {
                0 => cap,
                1 => expression(
                    ir::ExprKind::Tuple(vec![cap]),
                    ValueType::Compound(b::TypeId::ANY),
                ),
                _ => expression(
                    ir::ExprKind::Call {
                        target: ir::DefinitionId(1),
                        type_arguments: vec![],
                        args: vec![cap],
                        order: vec![0],
                    },
                    ValueType::Int,
                ),
            };
            let errors = f.audit(vec![function(0, vec![], value)]).unwrap_err();
            assert!(
                errors
                    .iter()
                    .all(|error| error.source.as_deref() == Some("main.emel"))
            );
        }
    }
    #[test]
    fn with_rejects_a_captured_capability_returned_directly_or_through_a_generic_call() {
        for through_call in [false, true] {
            let mut f = Fixture::new();
            let mut value = f.closure(true);
            let mut functions = Vec::new();
            if through_call {
                let parameter = ir::Local {
                    id: ir::LocalId(20),
                    name: "value".into(),
                    ty: data::frontend_type(f.core.types.intern(b::Type::Param(0))),
                };
                let mut identity = function(1, vec![parameter.clone()], reference(&parameter));
                identity.type_parameters = 1;
                functions.push(identity);
                value = expression(
                    ir::ExprKind::Call {
                        target: ir::DefinitionId(1),
                        type_arguments: vec![data::backend_type(f.callback)],
                        args: vec![value],
                        order: vec![0],
                    },
                    f.callback,
                );
            }
            functions.push(function(0, vec![], f.scope(value, true)));
            let errors = f.audit(functions).unwrap_err();
            assert!(
                errors
                    .iter()
                    .any(|error| error.message.contains("with scope"))
            );
        }
    }
    #[test]
    fn immediate_callback_calls_and_state_without_retirement_can_keep_captures() {
        let f = Fixture::new();
        let call = expression(
            ir::ExprKind::Apply {
                callee: Box::new(f.closure(true)),
                args: vec![],
                order: vec![],
            },
            ValueType::Int,
        );
        let body = f.scope(call, true);
        f.audit(vec![function(0, vec![], body)]).unwrap();
        let f = Fixture::new();
        let body = f.scope(f.closure(true), false);
        f.audit(vec![function(0, vec![], body)]).unwrap();
    }
    #[test]
    fn callback_return_summaries_distinguish_a_new_pure_closure_from_a_retained_capability() {
        for retained in [false, true] {
            let mut f = Fixture::new();
            let maker_type = Fixture::fn_type(&mut f.core, data::backend_type(f.callback));
            let parameter = ir::Local {
                id: ir::LocalId(20),
                name: "make".into(),
                ty: maker_type,
            };
            let invoke = function(
                1,
                vec![parameter.clone()],
                expression(
                    ir::ExprKind::Apply {
                        callee: Box::new(reference(&parameter)),
                        args: vec![],
                        order: vec![],
                    },
                    f.callback,
                ),
            );
            let maker = f.lambda(f.closure(retained), maker_type, true);
            let call = expression(
                ir::ExprKind::Call {
                    target: ir::DefinitionId(1),
                    type_arguments: vec![],
                    args: vec![maker],
                    order: vec![0],
                },
                f.callback,
            );
            let body = f.scope(call, true);
            assert_eq!(
                f.audit(vec![function(0, vec![], body), invoke]).is_err(),
                retained
            );
        }
    }

    #[test]
    fn a_capture_from_an_outer_provider_can_cross_an_inner_with_and_run_before_retirement() {
        let f = Fixture::new();
        let mut inner = f.scope(f.closure(true), true);
        let ir::ExprKind::With { bindings, body } = &mut inner.kind else {
            unreachable!()
        };
        bindings[0].local.id = ir::LocalId(11);
        let ir::Statement::Pattern { pattern, .. } = &mut body.statements[0] else {
            unreachable!()
        };
        let ir::PatternKind::Bind(local) = &mut pattern.kind else {
            unreachable!()
        };
        local.id = ir::LocalId(1);
        let call = expression(
            ir::ExprKind::Apply {
                callee: Box::new(inner),
                args: vec![],
                order: vec![],
            },
            ValueType::Int,
        );
        let body = f.scope(call, true);
        f.audit(vec![function(0, vec![], body)]).unwrap();
    }

    #[test]
    fn returned_closures_from_effectful_functions_retain_the_callers_provider() {
        let f = Fixture::new();
        let getter = function(
            1,
            vec![],
            expression(
                ir::ExprKind::Block(ir::Block {
                    ty: f.callback,
                    span: span(),
                    statements: vec![
                        ir::Statement::Pattern {
                            pattern: ir::Pattern {
                                kind: ir::PatternKind::Bind(f.cap.clone()),
                                ty: f.cap.ty,
                                span: span(),
                            },
                            value: expression(ir::ExprKind::Use { effect: f.effect }, f.cap.ty),
                        },
                        ir::Statement::Expr(f.closure(true)),
                    ],
                }),
                f.callback,
            ),
        );
        let call = expression(
            ir::ExprKind::Call {
                target: ir::DefinitionId(1),
                type_arguments: vec![],
                args: vec![],
                order: vec![],
            },
            f.callback,
        );
        let body = f.scope(call, true);
        let errors = f
            .audit(vec![function(0, vec![], body), getter])
            .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("with scope"))
        );
    }

    #[test]
    fn an_aggregate_cannot_hide_an_escaping_capability_capture() {
        let mut f = Fixture::new();
        let record = f.core.enums.alloc(b::EnumDef {
            name: "Saved".into(),
            params: 0,
            error: false,
            external: false,
            variants: vec![b::VariantDef {
                name: "Saved".into(),
                fields: b::VariantFields::Positional(1),
                tys: vec![data::backend_type(f.callback)],
            }],
        });
        let record_ty = data::frontend_type(f.core.types.enum_type(record, vec![]));
        let value = expression(
            ir::ExprKind::Make {
                ctor: b::CtorRef {
                    enum_id: record,
                    variant: 0,
                },
                args: vec![f.closure(true)],
                order: vec![0],
            },
            record_ty,
        );
        let body = f.scope(value, true);
        assert!(
            f.audit(vec![function(0, vec![], body)])
                .unwrap_err()
                .iter()
                .any(|error| error.message.contains("with scope"))
        );
    }

    #[test]
    fn source_capability_values_are_rejected_after_normalized_type_inference() {
        for body in ["use Read", "cap=use Read\nidentity(cap)", "[use Read]"] {
            let source = format!(
                "effect Read {{ fn read()->Int }}\nfn identity[A](x:A)->A {{x}}\nfn invalid() {{{body}}}"
            );
            let mut modules = super::super::modules::collect(
                &[crate::Source::new("main.emel", &source)],
                "main.emel",
            )
            .unwrap();
            super::super::consts::expand(&mut modules).unwrap();
            assert!(super::super::check::check(modules).is_err(), "{body}");
        }
    }

    #[test]
    fn ordinary_source_closure_ownership_fixtures_need_no_capability_lifetime() {
        let mut modules = super::super::modules::collect(
            &[crate::Source::new(
                "main.emel",
                include_str!("../../tests/compat/p4/closures.emel"),
            )],
            "main.emel",
        )
        .unwrap();
        super::super::consts::expand(&mut modules).unwrap();
        let program = super::super::check::check(modules).unwrap();
        audit(&program).unwrap();
    }
}
