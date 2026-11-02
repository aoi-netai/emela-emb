use super::*;
use crate::{
    contract::ir as b,
    frontend::{
        modules::Modules,
        types::{InvocationSets, Signature, ValueType as Ty},
    },
};
fn span() -> Span {
    Span::new(0, 1)
}
fn expr(kind: ir::ExprKind, ty: Ty) -> ir::Expr {
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
fn catalog() -> Catalog {
    Catalog::collect(&Modules {
        modules: vec![],
        entry: 0,
    })
    .unwrap()
}
fn effect(catalog: &mut Catalog, name: &str, args: Vec<TypeId>) -> (EffectId, TypeId) {
    let effect = catalog.core.effects.alloc(b::EffectDef {
        name: name.into(),
        decl: name.into(),
        args,
        label_of: None,
        ops: vec![b::OpDef {
            name: "get".into(),
            params: vec![],
            ret: TypeId::INT,
            fails: false,
            suspend: false,
            type_params: 0,
        }],
    });
    (effect, catalog.core.types.intern(Type::Handler(effect)))
}
fn get(effect: EffectId, ty: TypeId) -> ir::Expr {
    expr(
        ir::ExprKind::OpCall {
            handler: Box::new(expr(ir::ExprKind::Use { effect }, Ty::Compound(ty))),
            effect,
            op: 0,
            errors: vec![],
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    )
}
fn function(id: usize, value: ir::Expr) -> ir::Function {
    ir::Function {
        id: ir::DefinitionId(id),
        type_parameters: 0,
        immediate: false,
        constant: false,
        origin: None,
        derivation: None,
        error_show: None,
        name: format!("f{id}"),
        source_name: "test.emel".into(),
        span: span(),
        exported: false,
        export_symbol: None,
        signature: Signature {
            params: vec![],
            ret: Some(value.ty),
            sets: InvocationSets::default(),
        },
        params: vec![],
        external: None,
        intrinsic: None,
        body: Some(block(value)),
    }
}
fn program(catalog: &Catalog, functions: Vec<ir::Function>) -> ir::Program {
    ir::Program {
        core: catalog.core.clone(),
        registry: Default::default(),
        functions,
    }
}
fn call(id: usize) -> ir::Expr {
    expr(
        ir::ExprKind::Call {
            target: ir::DefinitionId(id),
            type_arguments: vec![],
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    )
}
fn pure_type(catalog: &mut Catalog) -> Ty {
    Ty::Compound(catalog.core.types.intern(Type::Fn(b::FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    })))
}
fn handler(effect: EffectId, ty: TypeId, operation: ir::Expr, operation_type: Ty) -> ir::Expr {
    expr(
        ir::ExprKind::Handler {
            name: "Provider".into(),
            effect,
            operations: vec![expr(
                ir::ExprKind::Lambda {
                    params: vec![],
                    captures: vec![],
                    body: block(operation),
                },
                operation_type,
            )],
        },
        Ty::Compound(ty),
    )
}
fn binding(id: usize, effect: EffectId, value: ir::Expr) -> ir::WithBinding {
    ir::WithBinding {
        local: ir::Local {
            id: ir::LocalId(id),
            name: "provider".into(),
            ty: value.ty,
        },
        value,
        effects: vec![effect],
        cleanup: None,
        retire: Some("Provider".into()),
    }
}

#[test]
fn recursive_calls_reach_a_fixed_point_and_explicit_use_is_checked() {
    let mut catalog = catalog();
    let (read, ty) = effect(&mut catalog, "Read", vec![]);
    let body = expr(
        ir::ExprKind::Block(ir::Block {
            statements: vec![
                ir::Statement::Expr(call(0)),
                ir::Statement::Expr(get(read, ty)),
            ],
            ty: Ty::Int,
            span: span(),
        }),
        Ty::Int,
    );
    let mut program = program(&catalog, vec![function(0, call(1)), function(1, body)]);
    infer(&mut program, &HashMap::new(), &mut catalog).unwrap();
    assert!(
        program
            .functions
            .iter()
            .all(|f| f.signature.sets.effects == [ty])
    );
    let modes = HashMap::from([(
        ir::DefinitionId(0),
        Mode {
            allowed: Some(vec![]),
        },
    )]);
    program.functions[0].signature.sets.effects.clear();
    assert!(
        infer(&mut program, &modes, &mut catalog).unwrap_err()[0]
            .message
            .contains("undeclared effect")
    );
}

#[test]
fn lambda_creation_is_pure_but_invocation_requires_its_effects() {
    let mut catalog = catalog();
    let (read, ty) = effect(&mut catalog, "Read", vec![]);
    let empty = catalog.core.types.intern(Type::Set(SetSummary::default()));
    let effects = catalog.core.types.intern(Type::Set(SetSummary {
        params: vec![ty],
        ..Default::default()
    }));
    let ft = Ty::Compound(catalog.core.types.intern(Type::Fn(b::FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: true,
        sets: Some(b::FnSets {
            errors: empty,
            effects,
        }),
    })));
    let lambda = expr(
        ir::ExprKind::Lambda {
            params: vec![],
            captures: vec![],
            body: block(get(read, ty)),
        },
        ft,
    );
    let invoke = expr(
        ir::ExprKind::Apply {
            callee: Box::new(lambda.clone()),
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    );
    let mut program = program(&catalog, vec![function(0, lambda), function(1, invoke)]);
    infer(&mut program, &HashMap::new(), &mut catalog).unwrap();
    assert!(program.functions[0].signature.sets.effects.is_empty());
    assert_eq!(program.functions[1].signature.sets.effects, [ty]);
}

#[test]
fn handlers_capture_dependencies_and_with_providers_apply_in_acquisition_order() {
    let mut catalog = catalog();
    let (outer, ot) = effect(&mut catalog, "Outer", vec![]);
    let (inner, it) = effect(&mut catalog, "Inner", vec![]);
    let pure = pure_type(&mut catalog);
    let first = binding(
        0,
        outer,
        handler(
            outer,
            ot,
            expr(ir::ExprKind::Literal(ir::Literal::Int(42)), Ty::Int),
            pure,
        ),
    );
    let mut second = binding(1, inner, handler(inner, it, get(outer, ot), pure));
    second.cleanup = Some(get(outer, ot));
    let body = expr(
        ir::ExprKind::With {
            bindings: vec![first.clone(), second.clone()],
            body: block(get(inner, it)),
        },
        Ty::Int,
    );
    let wrong_order = expr(
        ir::ExprKind::With {
            bindings: vec![second, first],
            body: block(get(inner, it)),
        },
        Ty::Int,
    );
    let mut program = program(&catalog, vec![function(0, body), function(1, wrong_order)]);
    infer(&mut program, &HashMap::new(), &mut catalog).unwrap();
    assert!(program.functions[0].signature.sets.effects.is_empty());
    assert_eq!(program.functions[1].signature.sets.effects, [ot]);
}

#[test]
fn different_instances_are_not_removed_by_the_same_effect_declaration() {
    let mut catalog = catalog();
    let (ints, it) = effect(&mut catalog, "State", vec![TypeId::INT]);
    let (strings, st) = effect(&mut catalog, "State", vec![TypeId::STRING]);
    let pure = pure_type(&mut catalog);
    let value = handler(
        ints,
        it,
        expr(ir::ExprKind::Literal(ir::Literal::Int(0)), Ty::Int),
        pure,
    );
    let body = expr(
        ir::ExprKind::Provide {
            base: None,
            bindings: vec![(ints, value)],
            body: block(get(strings, st)),
        },
        Ty::Int,
    );
    let mut program = program(&catalog, vec![function(0, body)]);
    infer(&mut program, &HashMap::new(), &mut catalog).unwrap();
    assert_eq!(program.functions[0].signature.sets.effects, [st]);
}

#[test]
fn a_known_inferred_residual_is_removed_by_an_outer_provider() {
    let mut catalog = catalog();
    let (read, ty) = effect(&mut catalog, "Read", vec![]);
    let mut solver = super::super::infer::Solver::new();
    let row = solver
        .fresh_row(
            &mut catalog.core.types,
            super::super::sets::SetKind::Effect,
            super::super::sets::Origin::Inferred,
        )
        .unwrap();
    let actual = catalog.core.types.intern(Type::Set(SetSummary {
        params: vec![ty],
        ..Default::default()
    }));
    solver.register_effects(&catalog.core);
    solver
        .include_sets(
            &mut catalog.core.types,
            actual,
            row,
            super::super::sets::SetKind::Effect,
        )
        .unwrap();
    let errors = catalog.core.types.intern(Type::Set(SetSummary::default()));
    let effects = catalog.core.types.intern(Type::Set(SetSummary {
        params: vec![row],
        ..Default::default()
    }));
    let callback = catalog.core.types.intern(Type::Fn(b::FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: true,
        sets: Some(b::FnSets { errors, effects }),
    }));
    let call = expr(
        ir::ExprKind::Apply {
            callee: Box::new(expr(
                ir::ExprKind::Local(ir::LocalId(0)),
                Ty::Compound(callback),
            )),
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    );
    let pure = pure_type(&mut catalog);
    let provided = handler(
        read,
        ty,
        expr(ir::ExprKind::Literal(ir::Literal::Int(42)), Ty::Int),
        pure,
    );
    let value = expr(
        ir::ExprKind::Provide {
            base: None,
            bindings: vec![(read, provided)],
            body: block(call),
        },
        Ty::Int,
    );
    assert!(
        incoming(
            &value,
            &[],
            &Registry::default(),
            &mut catalog,
            &mut solver,
            &[],
            "test.emel"
        )
        .unwrap()
        .is_empty()
    );
}

#[test]
fn a_provider_does_not_subtract_unknown_members_of_a_set_parameter() {
    let mut catalog = catalog();
    let (read, ty) = effect(&mut catalog, "Read", vec![]);
    let row = catalog.core.types.intern(Type::Param(0));
    let empty = catalog.core.types.intern(Type::Set(SetSummary::default()));
    let effects = catalog.core.types.intern(Type::Set(SetSummary {
        params: vec![row],
        ..Default::default()
    }));
    let ft = Ty::Compound(catalog.core.types.intern(Type::Fn(b::FnType {
        params: vec![],
        ret: TypeId::INT,
        fails: false,
        suspends: false,
        ev: true,
        sets: Some(b::FnSets {
            errors: empty,
            effects,
        }),
    })));
    let callback = ir::Local {
        id: ir::LocalId(0),
        name: "f".into(),
        ty: ft,
    };
    let invoke = expr(
        ir::ExprKind::Apply {
            callee: Box::new(expr(ir::ExprKind::Local(callback.id), ft)),
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    );
    let pure = pure_type(&mut catalog);
    let provider = handler(
        read,
        ty,
        expr(ir::ExprKind::Literal(ir::Literal::Int(42)), Ty::Int),
        pure,
    );
    let mut function = function(
        0,
        expr(
            ir::ExprKind::Provide {
                base: None,
                bindings: vec![(read, provider)],
                body: block(invoke),
            },
            Ty::Int,
        ),
    );
    function.type_parameters = 1;
    function.params = vec![callback];
    function.signature.params = vec![ft];
    let mut program = program(&catalog, vec![function]);
    infer(&mut program, &HashMap::new(), &mut catalog).unwrap();
    assert_eq!(program.functions[0].signature.sets.effects, [row]);
}

#[test]
fn undeclared_ambient_effects_and_effectful_const_initializers_are_rejected() {
    let mut catalog = catalog();
    let (read, ty) = effect(&mut catalog, "Read", vec![]);
    let mut program = program(&catalog, vec![function(0, get(read, ty))]);
    let modes = HashMap::from([(
        ir::DefinitionId(0),
        Mode {
            allowed: Some(vec![]),
        },
    )]);
    let mut solver = super::super::infer::Solver::new();
    assert!(constrain(&mut program, &modes, &mut catalog, &mut solver, &[vec![]]).is_err());
    program.functions[0].constant = true;
    assert!(infer(&mut program, &HashMap::new(), &mut catalog).is_err());
}

#[test]
fn calls_instantiate_effect_arguments_from_a_scoped_signature_scheme() {
    use crate::frontend::{ast, lexer, modules::Module, parser};
    let modules = Modules {
        modules: vec![Module {
            name: "Main".into(),
            source_name: "main.emel".into(),
            builtin: false,
            ast: parser::parse(&lexer::lex("pub effect State[S] { fn get()->S }").unwrap())
                .unwrap(),
        }],
        entry: 0,
    };
    let mut catalog = Catalog::collect(&modules).unwrap();
    let mut solver = super::super::infer::Solver::new();
    let parameter = solver
        .register_rigid_parameter(&mut catalog.core.types, 100)
        .unwrap();
    let generic = catalog
        .effects
        .resolve_instance(
            &modules,
            &mut catalog.core,
            0,
            &ast::Text::new("State", span()),
            vec![parameter],
        )
        .unwrap();
    let capability = catalog.core.types.intern(Type::Handler(generic));
    let body = expr(
        ir::ExprKind::OpCall {
            handler: Box::new(expr(
                ir::ExprKind::Use { effect: generic },
                Ty::Compound(capability),
            )),
            effect: generic,
            op: 0,
            errors: vec![],
            args: vec![],
            order: vec![],
        },
        Ty::Compound(parameter),
    );
    let mut read = function(0, body);
    read.type_parameters = 1;
    read.signature.sets.effects = vec![capability];
    let call = expr(
        ir::ExprKind::Call {
            target: ir::DefinitionId(0),
            type_arguments: vec![TypeId::INT],
            args: vec![],
            order: vec![],
        },
        Ty::Int,
    );
    let mut program = program(&catalog, vec![read, function(1, call.clone())]);
    let schemes = vec![vec![parameter], vec![]];
    constrain(
        &mut program,
        &HashMap::new(),
        &mut catalog,
        &mut solver,
        &schemes,
    )
    .unwrap();
    let concrete = catalog
        .effects
        .resolve_instance(
            &modules,
            &mut catalog.core,
            0,
            &ast::Text::new("State", span()),
            vec![TypeId::INT],
        )
        .unwrap();
    let concrete = catalog.core.types.intern(Type::Handler(concrete));
    assert_eq!(program.functions[1].signature.sets.effects, [concrete]);
    assert_eq!(
        incoming(
            &call,
            &program.functions,
            &program.registry,
            &mut catalog,
            &mut solver,
            &schemes,
            "call.emel"
        )
        .unwrap(),
        [concrete]
    );
}
