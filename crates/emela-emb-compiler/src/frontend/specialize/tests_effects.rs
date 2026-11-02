use super::*;

#[test]
fn generic_attempt_values_cross_the_typed_fiber_schema_and_closure_lowering() {
    let mut core = b::Definitions::default();
    let parameter = core.types.intern(b::Type::Param(0));
    let generic = frontend_type(parameter);
    let cause = core.enums.alloc(b::EnumDef {
        name: "Std.Cause.Cause".into(),
        params: 0,
        variants: vec![],
        error: false,
        external: true,
    });
    let cause_type = core.types.intern(b::Type::Enum(cause, vec![]));
    let exit = core.enums.alloc(b::EnumDef {
        name: "Exit".into(),
        params: 1,
        variants: vec![
            b::VariantDef {
                name: "Success".into(),
                fields: b::VariantFields::Positional(1),
                tys: vec![parameter],
            },
            b::VariantDef {
                name: "Failure".into(),
                fields: b::VariantFields::Positional(1),
                tys: vec![cause_type],
            },
        ],
        error: false,
        external: false,
    });
    let exit_type = frontend_type(core.types.intern(b::Type::Enum(exit, vec![parameter])));
    let callback_type = frontend_type(core.types.intern(b::Type::Fn(b::FnType {
        params: vec![],
        ret: parameter,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    })));
    let callback = expression(
        ir::ExprKind::Lambda {
            params: vec![],
            captures: vec![ir::LocalId(0)],
            body: block(expression(ir::ExprKind::Local(ir::LocalId(0)), generic)),
        },
        callback_type,
    );
    let attempt = expression(
        ir::ExprKind::Fiber {
            op: b::FiberOp::Attempt {
                exit,
                suspends: false,
            },
            args: vec![callback],
            errors: vec![],
        },
        exit_type,
    );
    let body = expression(
        ir::ExprKind::Field {
            base: Box::new(attempt),
            ctor: b::CtorRef {
                enum_id: exit,
                variant: b::EXIT_SUCCESS,
            },
            index: 0,
        },
        generic,
    );
    let mut identity = function(0, "through_attempt", vec![local(0, generic)], generic, body);
    identity.type_parameters = 1;
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        call(0, vec![integer(42)], vec![b::TypeId::INT], ValueType::Int),
    ));
    execute_with_options(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![identity, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        &crate::Options::default().with_heap_bytes(4096),
    );
}

fn effect(core: &mut b::Definitions, name: &str) -> (b::EffectId, ValueType) {
    let effect = core.effects.alloc(b::EffectDef {
        name: name.into(),
        decl: name.into(),
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
    let handler = frontend_type(core.types.intern(b::Type::Handler(effect)));
    (effect, handler)
}
fn fn_type(core: &mut b::Definitions, ev: bool) -> ValueType {
    frontend_type(core.types.intern(b::Type::Fn(b::FnType {
        params: vec![],
        ret: b::TypeId::INT,
        fails: false,
        suspends: false,
        ev,
        sets: None,
    })))
}
fn reference(value: &ir::Local) -> ir::Expr {
    expression(ir::ExprKind::Local(value.id), value.ty)
}
fn lambda(body: ir::Expr, ty: ValueType) -> ir::Expr {
    expression(
        ir::ExprKind::Lambda {
            params: vec![],
            captures: vec![],
            body: block(body),
        },
        ty,
    )
}
fn handler(effect: b::EffectId, handler_ty: ValueType, operation: ir::Expr) -> ir::Expr {
    expression(
        ir::ExprKind::Handler {
            name: "Reader".into(),
            effect,
            operations: vec![operation],
        },
        handler_ty,
    )
}
fn operation(effect: b::EffectId, handler_ty: ValueType) -> ir::Expr {
    expression(
        ir::ExprKind::OpCall {
            handler: Box::new(expression(ir::ExprKind::Use { effect }, handler_ty)),
            effect,
            op: 0,
            errors: vec![],
            args: vec![],
            order: vec![],
        },
        ValueType::Int,
    )
}
fn provide(effect: b::EffectId, handler: ir::Expr, body: ir::Expr) -> ir::Expr {
    let ty = body.ty;
    expression(
        ir::ExprKind::Provide {
            bindings: vec![(effect, handler)],
            body: block(body),
        },
        ty,
    )
}
fn apply(callee: ir::Expr) -> ir::Expr {
    expression(
        ir::ExprKind::Apply {
            callee: Box::new(callee),
            args: vec![],
            order: vec![],
        },
        ValueType::Int,
    )
}

#[test]
fn invocation_evidence_reaches_direct_calls_function_values_and_lambdas() {
    let mut core = b::Definitions::default();
    let (read, ht) = effect(&mut core, "Read");
    let pure = fn_type(&mut core, false);
    let effectful = fn_type(&mut core, true);
    let mut get = function(0, "get", vec![], ValueType::Int, operation(read, ht));
    get.signature.sets.effects.push(backend_type(ht));
    let callback = local(0, effectful);
    let mut invoke = function(
        1,
        "invoke",
        vec![callback.clone()],
        ValueType::Int,
        apply(reference(&callback)),
    );
    invoke.signature.sets.effects.push(backend_type(ht));
    let callback = local(1, effectful);
    let handler_local = local(2, ht);
    let mut answer = exported(function(2, "answer", vec![], ValueType::Int, integer(0)));
    let invoke_call = call(1, vec![reference(&callback)], vec![], ValueType::Int);
    let inner = provide(
        read,
        handler(read, ht, lambda(integer(42), pure)),
        invoke_call,
    );
    let outer = expression(
        ir::ExprKind::With {
            bindings: vec![ir::WithBinding {
                local: handler_local,
                value: handler(read, ht, lambda(integer(7), pure)),
                effects: vec![read],
                cleanup: Some(call(3, vec![], vec![], ValueType::Unit)),
                retire: Some("Reader".into()),
            }],
            body: ir::Block {
                ty: ValueType::Int,
                span: span(),
                statements: vec![
                    ir::Statement::Pattern {
                        pattern: ir::Pattern {
                            kind: ir::PatternKind::Bind(callback.clone()),
                            ty: effectful,
                            span: span(),
                        },
                        value: lambda(call(0, vec![], vec![], ValueType::Int), effectful),
                    },
                    ir::Statement::Expr(inner),
                ],
            },
        },
        ValueType::Int,
    );
    answer.body = Some(block(outer));
    let mut release = function(
        3,
        "released",
        vec![],
        ValueType::Unit,
        expression(ir::ExprKind::Literal(ir::Literal::Unit), ValueType::Unit),
    );
    release.external = Some("released".into());
    release.body = None;
    execute_with_options(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![get, invoke, answer, release],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic int count;\nvoid released(void){count++;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||count!=i+1||!test_heap_empty())return 1;return 0;}\n",
        &crate::Options::default().with_heap_bytes(4096),
    );
}

#[test]
fn handler_operations_capture_dependency_evidence_at_construction() {
    let mut core = b::Definitions::default();
    let (read, ht) = effect(&mut core, "Read");
    let (other, other_ht) = effect(&mut core, "Other");
    let pure = fn_type(&mut core, false);
    let mut get = function(0, "get", vec![], ValueType::Int, operation(read, ht));
    get.signature.sets.effects.push(backend_type(ht));
    let dependent = handler(
        other,
        other_ht,
        lambda(call(0, vec![], vec![], ValueType::Int), pure),
    );
    let dependent = provide(
        read,
        handler(read, ht, lambda(integer(42), pure)),
        dependent,
    );
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        provide(other, dependent, operation(other, other_ht)),
    ));
    execute_with_options(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![get, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        &crate::Options::default().with_heap_bytes(4096),
    );
}

#[test]
fn generic_labeled_effects_specialize_signature_lookup_and_operation_types() {
    let mut core = b::Definitions::default();
    let parameter = core.types.intern(b::Type::Param(0));
    let (generic, _) = effect(&mut core, "Read");
    core.effects[generic].args = vec![parameter];
    core.effects[generic].ops[0].ret = parameter;
    let mut concrete = core.effects[generic].clone();
    concrete.args = vec![b::TypeId::INT];
    concrete.ops[0].ret = b::TypeId::INT;
    let concrete = core.effects.alloc(concrete);
    let mut alias = core.effects[generic].clone();
    alias.name = "Audit".into();
    alias.decl = "Audit".into();
    alias.label_of = Some(generic);
    let alias = core.effects.alloc(alias);
    let generic_ht = frontend_type(core.types.intern(b::Type::Handler(alias)));
    let mut concrete_alias = core.effects[alias].clone();
    concrete_alias.args = vec![b::TypeId::INT];
    concrete_alias.ops[0].ret = b::TypeId::INT;
    concrete_alias.label_of = Some(concrete);
    let concrete_alias = core.effects.alloc(concrete_alias);
    let ht = frontend_type(core.types.intern(b::Type::Handler(concrete_alias)));
    let pure = fn_type(&mut core, false);
    let mut fetch = function(
        0,
        "fetch",
        vec![],
        frontend_type(parameter),
        expression(
            ir::ExprKind::OpCall {
                handler: Box::new(expression(ir::ExprKind::Use { effect: alias }, generic_ht)),
                effect: alias,
                op: 0,
                errors: vec![],
                args: vec![],
                order: vec![],
            },
            frontend_type(parameter),
        ),
    );
    fetch.type_parameters = 1;
    fetch.signature.sets.effects.push(backend_type(generic_ht));
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        provide(
            concrete_alias,
            handler(concrete_alias, ht, lambda(integer(42), pure)),
            call(0, vec![], vec![b::TypeId::INT], ValueType::Int),
        ),
    ));
    let input = ir::Program {
        core,
        registry: Default::default(),
        functions: vec![fetch, answer],
    };
    let output = specialize(input.clone()).unwrap();
    let specialized = output
        .functions
        .iter()
        .find(|function| function.name.starts_with("fetch$specialized"))
        .unwrap();
    assert_eq!(specialized.signature.sets.effects, vec![backend_type(ht)]);
    execute_with_options(
        input,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(answer()!=42||!test_heap_empty())return 1;return 0;}\n",
        &crate::Options::default().with_heap_bytes(4096),
    );
}

#[test]
fn normalization_freezes_capabilities_inside_function_sets_and_composite_types() {
    let mut core = b::Definitions::default();
    let mut solver = crate::frontend::infer::Solver::new();
    let meta = solver.fresh(&mut core.types, crate::frontend::infer::Constraint::Any);
    let (read, _) = effect(&mut core, "Read");
    core.effects[read].args = vec![meta];
    core.effects[read].ops[0].ret = meta;
    let mut alias = core.effects[read].clone();
    alias.name = "Audit".into();
    alias.decl = "Audit".into();
    alias.label_of = Some(read);
    let alias = core.effects.alloc(alias);
    let ht = core.types.intern(b::Type::Handler(alias));
    let list = core.types.intern(b::Type::List(ht));
    let tuple = core.types.intern(b::Type::Tuple(vec![list]));
    let empty = core.types.intern(b::Type::Set(b::SetSummary::default()));
    let effects = core.types.intern(b::Type::Set(b::SetSummary {
        ev: true,
        params: vec![ht],
        ..Default::default()
    }));
    let ft = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![tuple],
        ret: ht,
        ev: true,
        fails: false,
        suspends: false,
        sets: Some(b::FnSets {
            errors: empty,
            effects,
        }),
    }));
    let parameter = local(0, frontend_type(tuple));
    let closure = expression(
        ir::ExprKind::Lambda {
            params: vec![parameter],
            captures: vec![],
            body: block(expression(
                ir::ExprKind::Use { effect: alias },
                frontend_type(ht),
            )),
        },
        frontend_type(ft),
    );
    let function = function(0, "capability", vec![], frontend_type(ft), closure);
    solver.unify(&mut core.types, meta, b::TypeId::INT).unwrap();
    let mut input = ir::Program {
        core,
        registry: Default::default(),
        functions: vec![function],
    };
    crate::frontend::normalize::normalize(&mut input, &mut solver, &[vec![]]).unwrap();
    let b::Type::Fn(ft) = input
        .core
        .ty(backend_type(input.functions[0].signature.ret.unwrap()))
    else {
        panic!()
    };
    let b::Type::Handler(frozen) = input.core.ty(ft.ret) else {
        panic!()
    };
    assert_ne!(*frozen, alias);
    assert_eq!(input.core.effects[*frozen].args, vec![b::TypeId::INT]);
    assert_eq!(input.core.effects[*frozen].ops[0].ret, b::TypeId::INT);
    let base = input.core.effects[*frozen].label_of.unwrap();
    assert_eq!(input.core.effects[base].args, vec![b::TypeId::INT]);
    assert_eq!(input.core.effects[base].ops[0].ret, b::TypeId::INT);
    let b::Type::Set(summary) = input.core.ty(ft.sets.unwrap().effects) else {
        panic!()
    };
    assert_eq!(summary.params, vec![ft.ret]);
    let b::Type::Tuple(fields) = input.core.ty(ft.params[0]) else {
        panic!()
    };
    assert_eq!(input.core.ty(fields[0]), &b::Type::List(ft.ret));
    let ir::ExprKind::Lambda { body, .. } =
        &match &input.functions[0].body.as_ref().unwrap().statements[0] {
            ir::Statement::Expr(value) => value,
            _ => panic!(),
        }
        .kind
    else {
        panic!()
    };
    let ir::Statement::Expr(value) = &body.statements[0] else {
        panic!()
    };
    assert!(matches!(value.kind, ir::ExprKind::Use { effect } if effect == *frozen));
}

#[test]
fn failed_acquisition_releases_only_successful_bindings_in_reverse_order() {
    let mut core = b::Definitions::default();
    let (read, ht) = effect(&mut core, "Read");
    let pure = fn_type(&mut core, false);
    let (error, error_ty) = error_definition(&mut core, "FailedInit");
    let mut bad = function(
        0,
        "bad_init",
        vec![],
        ht,
        expression(
            ir::ExprKind::Fail(Box::new(error_value(error, error_ty, 42))),
            ValueType::Never,
        ),
    );
    bad.signature.sets.errors.push(error_ty);
    let mut mark = function(
        1,
        "mark",
        vec![local(0, ValueType::Int)],
        ValueType::Unit,
        expression(ir::ExprKind::Literal(ir::Literal::Unit), ValueType::Unit),
    );
    mark.external = Some("mark".into());
    mark.body = None;
    let mut bindings = Vec::new();
    for number in 1..=3 {
        bindings.push(ir::WithBinding {
            local: local(number, ht),
            value: if number == 3 {
                call(0, vec![], vec![], ht)
            } else {
                handler(read, ht, lambda(integer(7), pure))
            },
            effects: vec![read],
            cleanup: Some(call(
                1,
                vec![integer(number as i32)],
                vec![],
                ValueType::Unit,
            )),
            retire: Some(format!("Reader{number}")),
        });
    }
    let with = expression(
        ir::ExprKind::With {
            bindings,
            body: block(integer(0)),
        },
        ValueType::Int,
    );
    let escaped = expression(
        ir::ExprKind::Escape {
            value: Box::new(with),
            error: local(10, ValueType::Any),
            arms: vec![error_arm(
                error,
                error_ty,
                11,
                expression(ir::ExprKind::Local(ir::LocalId(11)), ValueType::Int),
            )],
        },
        ValueType::Int,
    );
    let answer = exported(function(2, "answer", vec![], ValueType::Int, escaped));
    execute_with_options(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![bad, mark, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nint test_heap_empty(void);\nstatic int order;\nvoid mark(int32_t n){order=order*10+n;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++){order=0;if(answer()!=42||order!=21||!test_heap_empty())return 1;}return 0;}\n",
        &crate::Options::default().with_heap_bytes(4096),
    );
}
