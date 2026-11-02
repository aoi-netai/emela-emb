use super::*;
use crate::frontend::{
    ast::Binary,
    types::{InvocationSets, Signature},
};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
fn span() -> Span {
    Span::new(10, 20)
}
fn expression(kind: ir::ExprKind, ty: ValueType) -> ir::Expr {
    ir::Expr {
        kind,
        ty,
        span: span(),
    }
}
fn local(id: usize, ty: ValueType) -> ir::Local {
    ir::Local {
        id: ir::LocalId(id),
        name: format!("local{id}"),
        ty,
    }
}
fn block(expr: ir::Expr) -> ir::Block {
    ir::Block {
        ty: expr.ty,
        statements: vec![ir::Statement::Expr(expr)],
        span: span(),
    }
}
fn function(
    id: usize,
    name: &str,
    params: Vec<ir::Local>,
    ret: ValueType,
    body: ir::Expr,
) -> ir::Function {
    ir::Function {
        id: ir::DefinitionId(id),
        type_parameters: 0,
        constant: false,
        origin: None,
        derivation: None,
        name: name.into(),
        source_name: format!("{name}.emel"),
        span: span(),
        exported: false,
        export_symbol: None,
        signature: Signature {
            params: params.iter().map(|local| local.ty).collect(),
            ret: Some(ret),
            sets: InvocationSets::default(),
        },
        params,
        external: None,
        intrinsic: None,
        body: Some(block(body)),
    }
}
fn call(
    target: usize,
    args: Vec<ir::Expr>,
    type_arguments: Vec<b::TypeId>,
    ty: ValueType,
) -> ir::Expr {
    let order = (0..args.len()).collect();
    expression(
        ir::ExprKind::Call {
            target: ir::DefinitionId(target),
            args,
            order,
            type_arguments,
        },
        ty,
    )
}
fn integer(value: i32) -> ir::Expr {
    expression(
        ir::ExprKind::Literal(ir::Literal::Int(value)),
        ValueType::Int,
    )
}
fn exported(mut function: ir::Function) -> ir::Function {
    function.exported = true;
    function.export_symbol = Some("answer".into());
    function
}
fn execute(input: ir::Program) {
    execute_with_harness(
        input,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {(void)k;(void)m;(void)n;exit(9);}\nint main(void) {return answer()==42 ? 0 : 1;}\n",
    );
}
fn execute_with_harness(input: ir::Program, harness: &str) {
    let program = specialize(input).unwrap();
    let lowered = crate::frontend::lower::lower(&program);
    let output = crate::backend::compile_program(lowered, &crate::Options::default()).unwrap();
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "emela-specialize-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    for file in output.files {
        std::fs::write(path.join(file.path), file.contents).unwrap();
    }
    std::fs::write(path.join("harness.c"), harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "emela_embedded.c",
                "harness.c",
                "-lm",
                "-o",
                "program",
            ])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(path.join("program")).output().unwrap();
        assert!(
            result.status.success(),
            "exit {:?}: {}",
            result.status.code(),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn one_generic_identity_becomes_independent_int_and_string_instances() {
    let mut core = b::Definitions::default();
    let param = core.types.intern(b::Type::Param(0));
    let generic = ValueType::Compound(param);
    let mut identity = function(
        0,
        "identity",
        vec![local(0, generic)],
        generic,
        expression(ir::ExprKind::Local(ir::LocalId(0)), generic),
    );
    identity.type_parameters = 1;
    let string_call = call(
        0,
        vec![expression(
            ir::ExprKind::Literal(ir::Literal::String("ok".into())),
            ValueType::String,
        )],
        vec![b::TypeId::STRING],
        ValueType::String,
    );
    let condition = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(string_call),
            right: Box::new(expression(
                ir::ExprKind::Literal(ir::Literal::String("ok".into())),
                ValueType::String,
            )),
        },
        ValueType::Bool,
    );
    let number = call(0, vec![integer(42)], vec![b::TypeId::INT], ValueType::Int);
    let body = expression(
        ir::ExprKind::If {
            condition: Box::new(condition),
            then: block(number),
            else_: block(integer(0)),
        },
        ValueType::Int,
    );
    let answer = exported(function(1, "answer", vec![], ValueType::Int, body));
    let input = ir::Program {
        registry: Default::default(),
        core,
        functions: vec![identity, answer],
    };
    let output = specialize(input.clone()).unwrap();
    assert_eq!(output.functions.len(), 3);
    assert!(
        output
            .functions
            .iter()
            .all(|function| function.type_parameters == 0)
    );
    assert!(
        output
            .functions
            .iter()
            .all(|function| function.id != ir::DefinitionId(0))
    );
    execute(input);
}
#[test]
fn tuple_patterns_match_local_and_expression_types_are_all_substituted() {
    let mut core = b::Definitions::default();
    let param = core.types.intern(b::Type::Param(0));
    let tuple = core
        .types
        .intern(b::Type::Tuple(vec![param, b::TypeId::UNIT]));
    let concrete = core
        .types
        .intern(b::Type::Tuple(vec![b::TypeId::INT, b::TypeId::UNIT]));
    let generic = ValueType::Compound(param);
    let tuple_ty = ValueType::Compound(tuple);
    let pattern = ir::Pattern {
        kind: ir::PatternKind::Tuple(vec![
            ir::Pattern {
                kind: ir::PatternKind::Bind(local(1, generic)),
                ty: generic,
                span: span(),
            },
            ir::Pattern {
                kind: ir::PatternKind::Any,
                ty: ValueType::Unit,
                span: span(),
            },
        ]),
        ty: tuple_ty,
        span: span(),
    };
    let body = expression(
        ir::ExprKind::Match {
            value: Box::new(expression(ir::ExprKind::Local(ir::LocalId(0)), tuple_ty)),
            arms: vec![ir::Arm {
                pattern,
                guard: None,
                value: expression(ir::ExprKind::Local(ir::LocalId(1)), generic),
            }],
        },
        generic,
    );
    let mut extract = function(0, "extract", vec![local(0, tuple_ty)], generic, body);
    extract.type_parameters = 1;
    let tuple = expression(
        ir::ExprKind::Tuple(vec![
            integer(42),
            expression(ir::ExprKind::Literal(ir::Literal::Unit), ValueType::Unit),
        ]),
        ValueType::Compound(concrete),
    );
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        call(0, vec![tuple], vec![b::TypeId::INT], ValueType::Int),
    ));
    execute(ir::Program {
        registry: Default::default(),
        core,
        functions: vec![extract, answer],
    });
}
fn recursive_input(growing: bool) -> ir::Program {
    let mut core = b::Definitions::default();
    let param = core.types.intern(b::Type::Param(0));
    let type_argument = if growing {
        core.types.intern(b::Type::Tuple(vec![param, param]))
    } else {
        param
    };
    let count = expression(ir::ExprKind::Local(ir::LocalId(0)), ValueType::Int);
    let condition = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(count.clone()),
            right: Box::new(integer(0)),
        },
        ValueType::Bool,
    );
    let decrement = expression(
        ir::ExprKind::Binary {
            op: Binary::Sub,
            left: Box::new(count),
            right: Box::new(integer(1)),
        },
        ValueType::Int,
    );
    let body = expression(
        ir::ExprKind::If {
            condition: Box::new(condition),
            then: block(integer(42)),
            else_: block(call(
                0,
                vec![decrement],
                vec![type_argument],
                ValueType::Int,
            )),
        },
        ValueType::Int,
    );
    let mut recursive = function(
        0,
        "recursive",
        vec![local(0, ValueType::Int)],
        ValueType::Int,
        body,
    );
    recursive.type_parameters = 1;
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        call(
            0,
            vec![integer(100000)],
            vec![b::TypeId::INT],
            ValueType::Int,
        ),
    ));
    ir::Program {
        registry: Default::default(),
        core,
        functions: vec![recursive, answer],
    }
}
#[test]
fn recursive_generic_call_reuses_its_placeholder_and_executes_without_stack_growth() {
    let input = recursive_input(false);
    let specialized = specialize(input.clone()).unwrap();
    assert_eq!(specialized.functions.len(), 2);
    execute(input);
}
#[test]
fn growing_type_arguments_and_malformed_calls_have_source_diagnostics() {
    let errors = specialize_with_limits(
        recursive_input(true),
        Limits {
            instances: 16,
            depth: 4,
            type_depth: 32,
        },
    )
    .unwrap_err();
    assert!(errors[0].message.contains("depth limit"));
    assert_eq!(errors[0].source.as_deref(), Some("recursive.emel"));
    assert_eq!(errors[0].span, span());
    let mut input = recursive_input(false);
    let answer = input.functions.last_mut().unwrap();
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::Call { type_arguments, .. },
        ..
    }) = &mut answer.body.as_mut().unwrap().statements[0]
    else {
        panic!()
    };
    type_arguments.clear();
    assert!(
        specialize(input).unwrap_err()[0]
            .message
            .contains("requires 1")
    );
}
#[test]
fn malformed_parameter_indices_and_evaluation_order_are_rejected() {
    let mut input = recursive_input(false);
    let invalid = input.core.types.intern(b::Type::Param(2));
    input.functions[0].signature.ret = Some(ValueType::Compound(invalid));
    assert!(
        specialize(input).unwrap_err()[0]
            .message
            .contains("unresolved type parameter 2")
    );
    let mut input = recursive_input(false);
    let answer = input.functions.last_mut().unwrap();
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::Call { order, .. },
        ..
    }) = &mut answer.body.as_mut().unwrap().statements[0]
    else {
        panic!()
    };
    order.push(0);
    assert!(
        specialize(input).unwrap_err()[0]
            .message
            .contains("complete permutation")
    );
}

#[test]
fn mutually_recursive_templates_share_reserved_concrete_instances() {
    let mut input = recursive_input(false);
    let mut odd = input.functions[0].clone();
    odd.id = ir::DefinitionId(2);
    odd.name = "odd".into();
    odd.source_name = "odd.emel".into();
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::If { else_, .. },
        ..
    }) = &mut input.functions[0].body.as_mut().unwrap().statements[0]
    else {
        panic!()
    };
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::Call { target, .. },
        ..
    }) = &mut else_.statements[0]
    else {
        panic!()
    };
    *target = ir::DefinitionId(2);
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::Call { args, .. },
        ..
    }) = &mut input.functions[1].body.as_mut().unwrap().statements[0]
    else {
        panic!()
    };
    args[0] = integer(64);
    input.functions.push(odd);
    let concrete = specialize(input.clone()).unwrap();
    assert_eq!(concrete.functions.len(), 3);
    assert!(
        concrete.functions.iter().all(
            |function| function.id != ir::DefinitionId(0) && function.id != ir::DefinitionId(2)
        )
    );
    execute(input);
}

#[test]
fn structural_trait_helpers_call_custom_fields_inside_tuple_option_and_enum() {
    use crate::frontend::traits::{BuiltinTrait as BT, Impl, Method, Registry, Trait, TypeHead};
    let mut core = b::Definitions::default();
    let ordering = core.enums.alloc(b::EnumDef {
        name: "Ordering".into(),
        params: 0,
        variants: ["Less", "Equal", "Greater"]
            .into_iter()
            .map(|name| b::VariantDef {
                name: name.into(),
                fields: b::VariantFields::Unit,
                tys: vec![],
            })
            .collect(),
        error: false,
        external: false,
    });
    let ordering_ty = core.types.intern(b::Type::Enum(ordering, vec![]));
    let self_param = core.types.intern(b::Type::Param(0));
    let mut registry = Registry::new();
    let mut trait_ids = HashMap::new();
    for (kind, name, method, ret, count) in [
        (BT::Eq, "Eq", "eq", b::TypeId::BOOL, 2),
        (BT::Ord, "Ord", "compare", ordering_ty, 2),
        (BT::Show, "Show", "show", b::TypeId::STRING, 1),
    ] {
        let signature = core.types.intern(b::Type::Fn(b::FnType {
            params: vec![self_param; count],
            ret,
            fails: false,
            suspends: false,
            ev: false,
            sets: None,
        }));
        let id = registry
            .add_trait(Trait {
                name: name.into(),
                owner_module: 0,
                supers: vec![],
                methods: vec![Method {
                    name: method.into(),
                    signature,
                    type_parameters: vec![],
                    default_function: None,
                }],
            })
            .unwrap();
        registry.set_builtin(id, kind).unwrap();
        trait_ids.insert(kind, id);
    }
    registry
        .set_supers(trait_ids[&BT::Ord], vec![trait_ids[&BT::Eq]])
        .unwrap();
    let key = core.enums.alloc(b::EnumDef {
        name: "Key".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Key".into(),
            fields: b::VariantFields::Named(vec!["value".into()]),
            tys: vec![b::TypeId::INT],
        }],
        error: false,
        external: false,
    });
    let key_ty = core.types.intern(b::Type::Enum(key, vec![]));
    let envelope = core.enums.alloc(b::EnumDef {
        name: "Envelope".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Envelope".into(),
            fields: b::VariantFields::Named(vec!["key".into(), "text".into()]),
            tys: vec![key_ty, b::TypeId::STRING],
        }],
        error: false,
        external: false,
    });
    let envelope_ty = core.types.intern(b::Type::Enum(envelope, vec![]));
    let signal = core.enums.alloc(b::EnumDef {
        name: "Signal".into(),
        params: 0,
        variants: vec![
            b::VariantDef {
                name: "Missing".into(),
                fields: b::VariantFields::Unit,
                tys: vec![],
            },
            b::VariantDef {
                name: "Present".into(),
                fields: b::VariantFields::Positional(2),
                tys: vec![key_ty, b::TypeId::STRING],
            },
        ],
        error: false,
        external: false,
    });
    let signal_ty = core.types.intern(b::Type::Enum(signal, vec![]));
    let option = core.enums.alloc(b::EnumDef {
        name: "Option".into(),
        params: 1,
        variants: vec![
            b::VariantDef {
                name: "None".into(),
                fields: b::VariantFields::Unit,
                tys: vec![],
            },
            b::VariantDef {
                name: "Some".into(),
                fields: b::VariantFields::Positional(1),
                tys: vec![self_param],
            },
        ],
        error: false,
        external: false,
    });
    let option_ty = core.types.intern(b::Type::Enum(option, vec![key_ty]));
    registry.register_option_type(&core, option).unwrap();
    let pair_ty = core
        .types
        .intern(b::Type::Tuple(vec![key_ty, b::TypeId::STRING]));
    let display_ty = core
        .types
        .intern(b::Type::Tuple(vec![envelope_ty, option_ty, signal_ty]));
    let key_front = frontend_type(key_ty);
    let a = local(0, key_front);
    let b_ = local(1, key_front);
    let field = |local: &ir::Local| {
        expression(
            ir::ExprKind::Field {
                base: Box::new(expression(ir::ExprKind::Local(local.id), local.ty)),
                ctor: b::CtorRef {
                    enum_id: key,
                    variant: 0,
                },
                index: 0,
            },
            ValueType::Int,
        )
    };
    let remainder = |value: ir::Expr| {
        expression(
            ir::ExprKind::Binary {
                op: Binary::Rem,
                left: Box::new(value),
                right: Box::new(integer(10)),
            },
            ValueType::Int,
        )
    };
    let eq_body = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(remainder(field(&a))),
            right: Box::new(remainder(field(&b_))),
        },
        ValueType::Bool,
    );
    let eq = function(
        0,
        "key_eq",
        vec![a.clone(), b_.clone()],
        ValueType::Bool,
        eq_body,
    );
    let trait_call = |kind: BT, self_type, args: Vec<ir::Expr>, ty| {
        expression(
            ir::ExprKind::TraitCall {
                trait_id: trait_ids[&kind],
                method: match kind {
                    BT::Eq => "eq",
                    BT::Ord => "compare",
                    BT::Show => "show",
                }
                .into(),
                self_type,
                order: (0..args.len()).collect(),
                args,
                method_type_arguments: vec![],
                nested_show: false,
            },
            ty,
        )
    };
    let ord = function(
        1,
        "key_ord",
        vec![a.clone(), b_],
        frontend_type(ordering_ty),
        trait_call(
            BT::Ord,
            b::TypeId::INT,
            vec![remainder(field(&a)), remainder(field(&local(1, key_front)))],
            frontend_type(ordering_ty),
        ),
    );
    let show = function(
        2,
        "key_show",
        vec![a.clone()],
        ValueType::String,
        expression(
            ir::ExprKind::Concat(vec![
                expression(
                    ir::ExprKind::Literal(ir::Literal::String("K".into())),
                    ValueType::String,
                ),
                expression(
                    ir::ExprKind::Builtin {
                        op: b::Builtin::IntToString,
                        args: vec![field(&a)],
                    },
                    ValueType::String,
                ),
            ]),
            ValueType::String,
        ),
    );
    registry.register_type_owner(key, 1).unwrap();
    for (kind, name, id) in [
        (BT::Eq, "eq", 0),
        (BT::Ord, "compare", 1),
        (BT::Show, "show", 2),
    ] {
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(key),
                    trait_id: trait_ids[&kind],
                    owner_module: 1,
                    type_parameters: vec![],
                    methods: HashMap::from([(name.into(), ir::DefinitionId(id))]),
                },
            )
            .unwrap();
    }
    let mut functions = vec![eq, ord, show];
    for (enum_id, ty, prefix) in [
        (envelope, envelope_ty, "envelope"),
        (signal, signal_ty, "signal"),
    ] {
        registry.register_type_owner(enum_id, 1).unwrap();
        for (kind, method, ret, count) in [
            (BT::Eq, "eq", ValueType::Bool, 2),
            (BT::Ord, "compare", frontend_type(ordering_ty), 2),
            (BT::Show, "show", ValueType::String, 1),
        ] {
            let id = functions.len();
            let mut derive = function(
                id,
                &format!("{prefix}_{method}"),
                (0..count).map(|id| local(id, frontend_type(ty))).collect(),
                ret,
                integer(0),
            );
            derive.body = None;
            derive.derivation = Some(kind);
            functions.push(derive);
            registry
                .add_impl(
                    &core,
                    Impl {
                        head: TypeHead::Enum(enum_id),
                        trait_id: trait_ids[&kind],
                        owner_module: 1,
                        type_parameters: vec![],
                        methods: HashMap::from([(method.into(), ir::DefinitionId(id))]),
                    },
                )
                .unwrap();
        }
    }
    let make = |ty, enum_id, variant, args: Vec<ir::Expr>| {
        expression(
            ir::ExprKind::Make {
                ctor: b::CtorRef { enum_id, variant },
                order: (0..args.len()).collect(),
                args,
            },
            frontend_type(ty),
        )
    };
    let key_value = |value| make(key_ty, key, 0, vec![integer(value)]);
    let text = || {
        expression(
            ir::ExprKind::Literal(ir::Literal::String("x".into())),
            ValueType::String,
        )
    };
    let record = |value| make(envelope_ty, envelope, 0, vec![key_value(value), text()]);
    let some = |value| make(option_ty, option, 1, vec![key_value(value)]);
    let present = |value| make(signal_ty, signal, 1, vec![key_value(value), text()]);
    let constant_id = functions.len();
    let mut constant = function(constant_id, "KEY_CONST", vec![], key_front, key_value(7));
    constant.constant = true;
    constant.origin = Some(constant.id);
    functions.push(constant);
    let constant_value = call(constant_id, vec![], vec![], key_front);
    let constant_pattern = ir::Pattern {
        kind: ir::PatternKind::Constant {
            value: Box::new(constant_value),
            coverage: None,
            comparison: None,
        },
        ty: key_front,
        span: span(),
    };
    let constant_match = expression(
        ir::ExprKind::Match {
            value: Box::new(key_value(17)),
            arms: vec![
                ir::Arm {
                    pattern: constant_pattern,
                    guard: None,
                    value: integer(42),
                },
                ir::Arm {
                    pattern: ir::Pattern {
                        kind: ir::PatternKind::Any,
                        ty: key_front,
                        span: span(),
                    },
                    guard: None,
                    value: integer(0),
                },
            ],
        },
        ValueType::Int,
    );
    let constant_ok = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(constant_match),
            right: Box::new(integer(42)),
        },
        ValueType::Bool,
    );
    let mut statements = vec![ir::Statement::Expr(expression(
        ir::ExprKind::Assert(Box::new(constant_ok)),
        ValueType::Unit,
    ))];
    for (ty, left, right) in [
        (envelope_ty, record(7), record(17)),
        (signal_ty, present(7), present(17)),
        (option_ty, some(7), some(17)),
        (
            pair_ty,
            expression(
                ir::ExprKind::Tuple(vec![key_value(7), text()]),
                frontend_type(pair_ty),
            ),
            expression(
                ir::ExprKind::Tuple(vec![key_value(17), text()]),
                frontend_type(pair_ty),
            ),
        ),
    ] {
        statements.push(ir::Statement::Expr(expression(
            ir::ExprKind::Assert(Box::new(trait_call(
                BT::Eq,
                ty,
                vec![left, right],
                ValueType::Bool,
            ))),
            ValueType::Unit,
        )));
    }
    let ordering_value = trait_call(
        BT::Ord,
        envelope_ty,
        vec![record(7), record(17)],
        frontend_type(ordering_ty),
    );
    let equal = ir::Pattern {
        kind: ir::PatternKind::Constructor {
            ctor: b::CtorRef {
                enum_id: ordering,
                variant: 1,
            },
            fields: vec![],
        },
        ty: frontend_type(ordering_ty),
        span: span(),
    };
    let compare = expression(
        ir::ExprKind::Match {
            value: Box::new(ordering_value),
            arms: vec![
                ir::Arm {
                    pattern: equal,
                    guard: None,
                    value: expression(
                        ir::ExprKind::Literal(ir::Literal::Bool(true)),
                        ValueType::Bool,
                    ),
                },
                ir::Arm {
                    pattern: ir::Pattern {
                        kind: ir::PatternKind::Any,
                        ty: frontend_type(ordering_ty),
                        span: span(),
                    },
                    guard: None,
                    value: expression(
                        ir::ExprKind::Literal(ir::Literal::Bool(false)),
                        ValueType::Bool,
                    ),
                },
            ],
        },
        ValueType::Bool,
    );
    statements.push(ir::Statement::Expr(expression(
        ir::ExprKind::Assert(Box::new(compare)),
        ValueType::Unit,
    )));
    let displayed = trait_call(
        BT::Show,
        display_ty,
        vec![expression(
            ir::ExprKind::Tuple(vec![record(7), some(7), present(7)]),
            frontend_type(display_ty),
        )],
        ValueType::String,
    );
    let expected = expression(
        ir::ExprKind::Literal(ir::Literal::String(
            "(Envelope(key: K7, text: \"x\"), Some(K7), Present(K7, \"x\"))".into(),
        )),
        ValueType::String,
    );
    let matches = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(displayed),
            right: Box::new(expected),
        },
        ValueType::Bool,
    );
    statements.push(ir::Statement::Expr(expression(
        ir::ExprKind::Assert(Box::new(matches)),
        ValueType::Unit,
    )));
    statements.push(ir::Statement::Expr(integer(42)));
    let mut answer = exported(function(
        functions.len(),
        "answer",
        vec![],
        ValueType::Int,
        integer(0),
    ));
    answer.body = Some(ir::Block {
        statements,
        ty: ValueType::Int,
        span: span(),
    });
    functions.push(answer);
    execute(ir::Program {
        core,
        registry,
        functions,
    });
}

#[test]
fn generic_trait_calls_instantiate_default_methods_with_the_concrete_self_type() {
    use crate::frontend::traits::{Impl, Method, Registry, Trait, TypeHead};
    let mut core = b::Definitions::default();
    let parameter = core.types.intern(b::Type::Param(0));
    let generic = frontend_type(parameter);
    let key = core.enums.alloc(b::EnumDef {
        name: "Key".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Key".into(),
            fields: b::VariantFields::Named(vec!["value".into()]),
            tys: vec![b::TypeId::INT],
        }],
        error: false,
        external: false,
    });
    let key_ty = core.types.intern(b::Type::Enum(key, vec![]));
    let signature = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![parameter],
        ret: b::TypeId::INT,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut registry = Registry::new();
    let score = registry
        .add_trait(Trait {
            name: "Score".into(),
            owner_module: 0,
            supers: vec![],
            methods: vec![
                Method {
                    name: "score".into(),
                    signature,
                    type_parameters: vec![],
                    default_function: None,
                },
                Method {
                    name: "next".into(),
                    signature,
                    type_parameters: vec![],
                    default_function: Some(ir::DefinitionId(1)),
                },
            ],
        })
        .unwrap();
    registry.register_type_owner(key, 1).unwrap();
    registry
        .add_impl(
            &core,
            Impl {
                head: TypeHead::Enum(key),
                trait_id: score,
                owner_module: 1,
                type_parameters: vec![],
                methods: HashMap::from([("score".into(), ir::DefinitionId(0))]),
            },
        )
        .unwrap();
    let key_param = local(0, frontend_type(key_ty));
    let value = expression(
        ir::ExprKind::Field {
            base: Box::new(expression(ir::ExprKind::Local(key_param.id), key_param.ty)),
            ctor: b::CtorRef {
                enum_id: key,
                variant: 0,
            },
            index: 0,
        },
        ValueType::Int,
    );
    let implementation = function(
        0,
        "score",
        vec![key_param],
        ValueType::Int,
        expression(
            ir::ExprKind::Binary {
                op: Binary::Add,
                left: Box::new(value),
                right: Box::new(integer(1)),
            },
            ValueType::Int,
        ),
    );
    let trait_call = |method: &str| {
        expression(
            ir::ExprKind::TraitCall {
                trait_id: score,
                method: method.into(),
                self_type: parameter,
                args: vec![expression(ir::ExprKind::Local(ir::LocalId(0)), generic)],
                order: vec![0],
                method_type_arguments: vec![],
                nested_show: false,
            },
            ValueType::Int,
        )
    };
    let mut default = function(
        1,
        "next",
        vec![local(0, generic)],
        ValueType::Int,
        expression(
            ir::ExprKind::Binary {
                op: Binary::Add,
                left: Box::new(trait_call("score")),
                right: Box::new(integer(1)),
            },
            ValueType::Int,
        ),
    );
    default.type_parameters = 1;
    let mut generic_call = function(
        2,
        "generic",
        vec![local(0, generic)],
        ValueType::Int,
        trait_call("next"),
    );
    generic_call.type_parameters = 1;
    let key_value = expression(
        ir::ExprKind::Make {
            ctor: b::CtorRef {
                enum_id: key,
                variant: 0,
            },
            args: vec![integer(7)],
            order: vec![0],
        },
        frontend_type(key_ty),
    );
    let next = call(2, vec![key_value], vec![key_ty], ValueType::Int);
    let answer = exported(function(
        3,
        "answer",
        vec![],
        ValueType::Int,
        expression(
            ir::ExprKind::Binary {
                op: Binary::Add,
                left: Box::new(next),
                right: Box::new(integer(33)),
            },
            ValueType::Int,
        ),
    ));
    execute(ir::Program {
        core,
        registry,
        functions: vec![implementation, default, generic_call, answer],
    });
}

fn error_definition(core: &mut b::Definitions, name: &str) -> (b::EnumId, b::TypeId) {
    let id = core.enums.alloc(b::EnumDef {
        name: name.into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: name.into(),
            fields: b::VariantFields::Positional(1),
            tys: vec![b::TypeId::INT],
        }],
        error: true,
        external: false,
    });
    let ty = core.types.intern(b::Type::Enum(id, vec![]));
    (id, ty)
}
fn error_value(id: b::EnumId, ty: b::TypeId, value: i32) -> ir::Expr {
    expression(
        ir::ExprKind::Make {
            ctor: b::CtorRef {
                enum_id: id,
                variant: 0,
            },
            args: vec![integer(value)],
            order: vec![0],
        },
        frontend_type(ty),
    )
}
fn error_arm(id: b::EnumId, ty: b::TypeId, binding: usize, value: ir::Expr) -> ir::Arm {
    ir::Arm {
        pattern: ir::Pattern {
            kind: ir::PatternKind::Constructor {
                ctor: b::CtorRef {
                    enum_id: id,
                    variant: 0,
                },
                fields: vec![ir::Pattern {
                    kind: ir::PatternKind::Bind(local(binding, ValueType::Int)),
                    ty: ValueType::Int,
                    span: span(),
                }],
            },
            ty: frontend_type(ty),
            span: span(),
        },
        guard: None,
        value,
    }
}
#[test]
fn escape_leaves_the_whole_expression_before_later_calls_execute() {
    let mut core = b::Definitions::default();
    let (error, error_ty) = error_definition(&mut core, "Written");
    let mut bad = function(
        0,
        "bad",
        vec![],
        ValueType::Int,
        expression(
            ir::ExprKind::Fail(Box::new(error_value(error, error_ty, 7))),
            ValueType::Never,
        ),
    );
    bad.signature.sets.errors = vec![error_ty];
    let mut after = function(1, "after", vec![], ValueType::Int, integer(1));
    after.body.as_mut().unwrap().statements.insert(
        0,
        ir::Statement::Expr(expression(
            ir::ExprKind::Assert(Box::new(expression(
                ir::ExprKind::Literal(ir::Literal::Bool(false)),
                ValueType::Bool,
            ))),
            ValueType::Unit,
        )),
    );
    let value = expression(
        ir::ExprKind::Binary {
            op: Binary::Add,
            left: Box::new(call(0, vec![], vec![], ValueType::Int)),
            right: Box::new(call(1, vec![], vec![], ValueType::Int)),
        },
        ValueType::Int,
    );
    let recovered = expression(
        ir::ExprKind::Binary {
            op: Binary::Add,
            left: Box::new(expression(
                ir::ExprKind::Local(ir::LocalId(11)),
                ValueType::Int,
            )),
            right: Box::new(integer(35)),
        },
        ValueType::Int,
    );
    let escaped = expression(
        ir::ExprKind::Escape {
            value: Box::new(value),
            error: local(10, ValueType::Any),
            arms: vec![error_arm(error, error_ty, 11, recovered)],
        },
        ValueType::Int,
    );
    let answer = exported(function(2, "answer", vec![], ValueType::Int, escaped));
    execute(ir::Program {
        core,
        registry: Default::default(),
        functions: vec![bad, after, answer],
    });
}
#[test]
fn local_fail_and_nested_escape_handlers_propagate_to_the_outer_scope() {
    let mut core = b::Definitions::default();
    let (first, first_ty) = error_definition(&mut core, "First");
    let (second, second_ty) = error_definition(&mut core, "Second");
    let fail_first = expression(
        ir::ExprKind::Fail(Box::new(error_value(first, first_ty, 7))),
        ValueType::Never,
    );
    let fail_second = expression(
        ir::ExprKind::Fail(Box::new(error_value(second, second_ty, 42))),
        ValueType::Never,
    );
    let inner = expression(
        ir::ExprKind::Escape {
            value: Box::new(fail_first),
            error: local(10, ValueType::Any),
            arms: vec![error_arm(first, first_ty, 11, fail_second)],
        },
        ValueType::Int,
    );
    let outer = expression(
        ir::ExprKind::Escape {
            value: Box::new(inner),
            error: local(20, ValueType::Any),
            arms: vec![error_arm(
                second,
                second_ty,
                21,
                expression(ir::ExprKind::Local(ir::LocalId(21)), ValueType::Int),
            )],
        },
        ValueType::Int,
    );
    let answer = exported(function(0, "answer", vec![], ValueType::Int, outer));
    execute(ir::Program {
        core,
        registry: Default::default(),
        functions: vec![answer],
    });
}
#[test]
fn generic_error_constraints_are_substituted_and_non_errors_are_rejected() {
    let mut core = b::Definitions::default();
    let (error, error_ty) = error_definition(&mut core, "Typed");
    let parameter = core.types.intern(b::Type::Param(0));
    let generic = frontend_type(parameter);
    let mut throw = function(
        0,
        "throw",
        vec![local(0, generic)],
        ValueType::Never,
        expression(
            ir::ExprKind::Fail(Box::new(expression(
                ir::ExprKind::Local(ir::LocalId(0)),
                generic,
            ))),
            ValueType::Never,
        ),
    );
    throw.type_parameters = 1;
    throw.signature.sets.errors = vec![parameter];
    throw.signature.sets.error_constraints = vec![parameter];
    let thrown = call(
        0,
        vec![error_value(error, error_ty, 42)],
        vec![error_ty],
        ValueType::Never,
    );
    let escaped = expression(
        ir::ExprKind::Escape {
            value: Box::new(thrown),
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
    let answer = exported(function(1, "answer", vec![], ValueType::Int, escaped));
    let input = ir::Program {
        core,
        registry: Default::default(),
        functions: vec![throw, answer],
    };
    execute(input.clone());
    let mut invalid = input;
    let ir::Statement::Expr(ir::Expr {
        kind: ir::ExprKind::Escape { value, .. },
        ..
    }) = &mut invalid.functions[1].body.as_mut().unwrap().statements[0]
    else {
        panic!()
    };
    let ir::ExprKind::Call {
        args,
        type_arguments,
        ..
    } = &mut value.kind
    else {
        panic!()
    };
    args[0] = integer(42);
    type_arguments[0] = b::TypeId::INT;
    assert!(
        specialize(invalid).unwrap_err()[0]
            .message
            .contains("concrete error type")
    );
}

#[test]
fn unmatched_guarded_errors_propagate_without_reentering_the_same_escape() {
    let mut core = b::Definitions::default();
    let (error, error_ty) = error_definition(&mut core, "Guarded");
    let mut arm = error_arm(error, error_ty, 11, integer(0));
    arm.guard = Some(expression(
        ir::ExprKind::Literal(ir::Literal::Bool(false)),
        ValueType::Bool,
    ));
    let failing = expression(
        ir::ExprKind::Fail(Box::new(error_value(error, error_ty, 42))),
        ValueType::Never,
    );
    let inner = expression(
        ir::ExprKind::Escape {
            value: Box::new(failing),
            error: local(10, ValueType::Any),
            arms: vec![arm],
        },
        ValueType::Int,
    );
    let mut partial = function(0, "partial", vec![], ValueType::Int, inner);
    partial.signature.sets.errors = vec![error_ty];
    let outer = expression(
        ir::ExprKind::Escape {
            value: Box::new(call(0, vec![], vec![], ValueType::Int)),
            error: local(20, ValueType::Any),
            arms: vec![error_arm(
                error,
                error_ty,
                21,
                expression(ir::ExprKind::Local(ir::LocalId(21)), ValueType::Int),
            )],
        },
        ValueType::Int,
    );
    let answer = exported(function(1, "answer", vec![], ValueType::Int, outer));
    execute(ir::Program {
        core,
        registry: Default::default(),
        functions: vec![partial, answer],
    });
}

fn constant_pattern(value: ir::Expr, ty: ValueType) -> ir::Pattern {
    ir::Pattern {
        kind: ir::PatternKind::Constant {
            value: Box::new(value),
            coverage: None,
            comparison: None,
        },
        ty,
        span: span(),
    }
}
fn constant_match(
    subject: ir::Expr,
    pattern: ir::Pattern,
    matched: ir::Expr,
    fallback: ir::Expr,
) -> ir::Expr {
    let ty = matched.ty;
    let subject_ty = subject.ty;
    expression(
        ir::ExprKind::Match {
            value: Box::new(subject),
            arms: vec![
                ir::Arm {
                    pattern,
                    guard: None,
                    value: matched,
                },
                ir::Arm {
                    pattern: ir::Pattern {
                        kind: ir::PatternKind::Any,
                        ty: subject_ty,
                        span: span(),
                    },
                    guard: None,
                    value: fallback,
                },
            ],
        },
        ty,
    )
}
#[test]
fn constant_pattern_getters_run_on_mismatch_and_keep_their_single_runtime_cache() {
    let core = b::Definitions::default();
    let mut read = function(0, "read_once", vec![], ValueType::Int, integer(0));
    read.body = None;
    read.external = Some("read_once".into());
    let mut constant = function(
        1,
        "MEMO",
        vec![],
        ValueType::Int,
        call(0, vec![], vec![], ValueType::Int),
    );
    constant.constant = true;
    constant.origin = Some(constant.id);
    let pattern = constant_pattern(call(1, vec![], vec![], ValueType::Int), ValueType::Int);
    let answer = exported(function(
        2,
        "answer",
        vec![],
        ValueType::Int,
        constant_match(integer(0), pattern, integer(0), integer(42)),
    ));
    execute_with_harness(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![read, constant, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int reads;\nint32_t read_once(void){++reads;return 7;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;exit(9);}\nint main(void){return answer()==42 && answer()==42 && reads==1 ? 0:1;}\n",
    );
}
#[test]
fn plain_data_constant_patterns_use_recursive_raw_structural_equality_without_eq() {
    let mut core = b::Definitions::default();
    let (inner, inner_ty) = error_definition(&mut core, "Inner");
    core.enums[inner].error = false;
    let outer = core.enums.alloc(b::EnumDef {
        name: "Outer".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Outer".into(),
            fields: b::VariantFields::Positional(1),
            tys: vec![inner_ty],
        }],
        error: false,
        external: false,
    });
    let outer_ty = core.types.intern(b::Type::Enum(outer, vec![]));
    let make = |number| {
        expression(
            ir::ExprKind::Make {
                ctor: b::CtorRef {
                    enum_id: outer,
                    variant: 0,
                },
                args: vec![error_value(inner, inner_ty, number)],
                order: vec![0],
            },
            frontend_type(outer_ty),
        )
    };
    let mut constant = function(0, "ORIGIN", vec![], frontend_type(outer_ty), make(42));
    constant.constant = true;
    constant.origin = Some(constant.id);
    let test = constant_match(
        make(43),
        constant_pattern(
            call(0, vec![], vec![], frontend_type(outer_ty)),
            frontend_type(outer_ty),
        ),
        integer(0),
        integer(42),
    );
    let matches = constant_match(
        make(42),
        constant_pattern(
            call(0, vec![], vec![], frontend_type(outer_ty)),
            frontend_type(outer_ty),
        ),
        integer(42),
        integer(0),
    );
    let condition = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(test),
            right: Box::new(integer(42)),
        },
        ValueType::Bool,
    );
    let body = expression(
        ir::ExprKind::If {
            condition: Box::new(condition),
            then: block(matches),
            else_: block(integer(0)),
        },
        ValueType::Int,
    );
    let answer = exported(function(1, "answer", vec![], ValueType::Int, body));
    execute(ir::Program {
        core,
        registry: Default::default(),
        functions: vec![constant, answer],
    });
}
#[test]
fn constant_pattern_coverage_only_expands_literal_constructor_tuple_and_alias_syntax() {
    use crate::frontend::constant_patterns::coverage;
    let mut original = function(
        0,
        "FLAG",
        vec![],
        ValueType::Bool,
        expression(
            ir::ExprKind::Literal(ir::Literal::Bool(false)),
            ValueType::Bool,
        ),
    );
    original.constant = true;
    let mut alias = function(
        1,
        "ALIAS",
        vec![],
        ValueType::Bool,
        call(0, vec![], vec![], ValueType::Bool),
    );
    alias.constant = true;
    let mut functions = vec![Some(original), Some(alias)];
    let flag = call(1, vec![], vec![], ValueType::Bool);
    assert!(matches!(
        coverage(&flag, &functions).unwrap().kind,
        ir::PatternKind::Literal(ir::Literal::Bool(false))
    ));
    functions[0].as_mut().unwrap().body = Some(block(call(1, vec![], vec![], ValueType::Bool)));
    assert!(coverage(&flag, &functions).is_none());
    let computed = expression(
        ir::ExprKind::Binary {
            op: Binary::Add,
            left: Box::new(integer(1)),
            right: Box::new(integer(2)),
        },
        ValueType::Int,
    );
    assert!(coverage(&computed, &[]).is_none());
    let negative = expression(
        ir::ExprKind::Unary {
            op: crate::frontend::ast::Unary::Neg,
            value: Box::new(integer(7)),
        },
        ValueType::Int,
    );
    assert!(matches!(
        coverage(&negative, &[]).unwrap().kind,
        ir::PatternKind::Literal(ir::Literal::Int(-7))
    ));
    let mut catalog = crate::frontend::data::Catalog::collect(&crate::frontend::modules::Modules {
        modules: vec![],
        entry: 0,
    })
    .unwrap();
    let false_value = expression(
        ir::ExprKind::Literal(ir::Literal::Bool(false)),
        ValueType::Bool,
    );
    let true_value = expression(
        ir::ExprKind::Literal(ir::Literal::Bool(true)),
        ValueType::Bool,
    );
    let mut false_pattern = constant_pattern(false_value.clone(), ValueType::Bool);
    let mut true_pattern = constant_pattern(true_value.clone(), ValueType::Bool);
    if let ir::PatternKind::Constant {
        coverage: covered, ..
    } = &mut false_pattern.kind
    {
        *covered = coverage(&false_value, &[]).map(Box::new);
    }
    if let ir::PatternKind::Constant {
        coverage: covered, ..
    } = &mut true_pattern.kind
    {
        *covered = coverage(&true_value, &[]).map(Box::new);
    }
    assert!(crate::frontend::patterns::exhaustive(
        &[false_pattern, true_pattern],
        ValueType::Bool,
        &mut catalog
    ));
    assert!(!crate::frontend::patterns::exhaustive(
        &[constant_pattern(true_value, ValueType::Bool)],
        ValueType::Bool,
        &mut catalog
    ));
}

#[test]
fn never_returning_constant_getters_terminate_before_pattern_comparison() {
    let core = b::Definitions::default();
    let mut stop = function(0, "stop", vec![], ValueType::Never, integer(0));
    stop.body = None;
    stop.external = Some("stop".into());
    let mut constant = function(
        1,
        "DEAD",
        vec![],
        ValueType::Never,
        call(0, vec![], vec![], ValueType::Never),
    );
    constant.constant = true;
    constant.origin = Some(constant.id);
    let pattern = constant_pattern(call(1, vec![], vec![], ValueType::Never), ValueType::Int);
    let answer = exported(function(
        2,
        "answer",
        vec![],
        ValueType::Int,
        constant_match(integer(0), pattern, integer(42), integer(0)),
    ));
    execute_with_harness(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![stop, constant, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void stop(void){exit(0);}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;exit(9);}\nint main(void){(void)answer();return 1;}\n",
    );
}

#[test]
fn nested_string_show_uses_quoting_even_when_a_builtin_method_stub_is_registered() {
    use crate::frontend::traits::{BuiltinTrait, Method, Registry, Trait};
    let mut core = b::Definitions::default();
    let parameter = core.types.intern(b::Type::Param(0));
    let signature = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![parameter],
        ret: b::TypeId::STRING,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut registry = Registry::new();
    let show = registry
        .add_trait(Trait {
            name: "Show".into(),
            owner_module: 0,
            supers: vec![],
            methods: vec![Method {
                name: "show".into(),
                signature,
                type_parameters: vec![],
                default_function: None,
            }],
        })
        .unwrap();
    registry.set_builtin(show, BuiltinTrait::Show).unwrap();
    registry.register_builtin_method(BuiltinTrait::Show, "show", ir::DefinitionId(0));
    let mut stub = function(
        0,
        "builtin_show",
        vec![local(0, frontend_type(parameter))],
        ValueType::String,
        integer(0),
    );
    stub.type_parameters = 1;
    stub.body = None;
    stub.derivation = Some(BuiltinTrait::Show);
    let show = expression(
        ir::ExprKind::TraitCall {
            trait_id: show,
            method: "show".into(),
            self_type: b::TypeId::STRING,
            args: vec![expression(
                ir::ExprKind::Literal(ir::Literal::String("x".into())),
                ValueType::String,
            )],
            order: vec![0],
            method_type_arguments: vec![],
            nested_show: true,
        },
        ValueType::String,
    );
    let expected = expression(
        ir::ExprKind::Literal(ir::Literal::String("\"x\"".into())),
        ValueType::String,
    );
    let condition = expression(
        ir::ExprKind::Binary {
            op: Binary::Eq,
            left: Box::new(show),
            right: Box::new(expected),
        },
        ValueType::Bool,
    );
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        expression(
            ir::ExprKind::If {
                condition: Box::new(condition),
                then: block(integer(42)),
                else_: block(integer(0)),
            },
            ValueType::Int,
        ),
    ));
    execute(ir::Program {
        core,
        registry,
        functions: vec![stub, answer],
    });
}

#[test]
fn dbg_uses_custom_show_and_recursive_fallback_and_returns_the_original_value_once() {
    use crate::frontend::traits::{BuiltinTrait, Impl, Method, Registry, Trait, TypeHead};
    let mut core = b::Definitions::default();
    let self_ty = core.types.intern(b::Type::Param(0));
    let signature = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![self_ty],
        ret: b::TypeId::STRING,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut registry = Registry::new();
    let show = registry
        .add_trait(Trait {
            name: "Show".into(),
            owner_module: 0,
            supers: vec![],
            methods: vec![Method {
                name: "show".into(),
                signature,
                type_parameters: vec![],
                default_function: None,
            }],
        })
        .unwrap();
    registry.set_builtin(show, BuiltinTrait::Show).unwrap();
    let (key, key_ty) = error_definition(&mut core, "Key");
    core.enums[key].error = false;
    let inner = core.enums.alloc(b::EnumDef {
        name: "Inner".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Inner".into(),
            fields: b::VariantFields::Named(vec!["text".into()]),
            tys: vec![b::TypeId::STRING],
        }],
        error: false,
        external: false,
    });
    let inner_ty = core.types.intern(b::Type::Enum(inner, vec![]));
    let outer = core.enums.alloc(b::EnumDef {
        name: "Outer".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Outer".into(),
            fields: b::VariantFields::Named(vec!["key".into(), "inner".into()]),
            tys: vec![key_ty, inner_ty],
        }],
        error: false,
        external: false,
    });
    let outer_ty = core.types.intern(b::Type::Enum(outer, vec![]));
    let mut read = function(0, "read_once", vec![], ValueType::Int, integer(0));
    read.body = None;
    read.external = Some("read_once".into());
    let show_key = function(
        1,
        "key_show",
        vec![local(0, frontend_type(key_ty))],
        ValueType::String,
        expression(
            ir::ExprKind::Literal(ir::Literal::String("custom".into())),
            ValueType::String,
        ),
    );
    registry.register_type_owner(key, 1).unwrap();
    registry
        .add_impl(
            &core,
            Impl {
                head: TypeHead::Enum(key),
                trait_id: show,
                owner_module: 1,
                type_parameters: vec![],
                methods: HashMap::from([("show".into(), ir::DefinitionId(1))]),
            },
        )
        .unwrap();
    let key_value = expression(
        ir::ExprKind::Make {
            ctor: b::CtorRef {
                enum_id: key,
                variant: 0,
            },
            args: vec![call(0, vec![], vec![], ValueType::Int)],
            order: vec![0],
        },
        frontend_type(key_ty),
    );
    let inner_value = expression(
        ir::ExprKind::Make {
            ctor: b::CtorRef {
                enum_id: inner,
                variant: 0,
            },
            args: vec![expression(
                ir::ExprKind::Literal(ir::Literal::String("x".into())),
                ValueType::String,
            )],
            order: vec![0],
        },
        frontend_type(inner_ty),
    );
    let outer_value = expression(
        ir::ExprKind::Make {
            ctor: b::CtorRef {
                enum_id: outer,
                variant: 0,
            },
            args: vec![key_value, inner_value],
            order: vec![0, 1],
        },
        frontend_type(outer_ty),
    );
    let marker = local(100, ValueType::Int);
    let result = local(101, frontend_type(outer_ty));
    let dbg = expression(
        ir::ExprKind::Builtin {
            op: b::Builtin::Dbg,
            args: vec![outer_value.clone()],
        },
        frontend_type(outer_ty),
    );
    let projected_key = expression(
        ir::ExprKind::Field {
            base: Box::new(expression(ir::ExprKind::Local(result.id), result.ty)),
            ctor: b::CtorRef {
                enum_id: outer,
                variant: 0,
            },
            index: 0,
        },
        frontend_type(key_ty),
    );
    let projected_value = expression(
        ir::ExprKind::Field {
            base: Box::new(projected_key),
            ctor: b::CtorRef {
                enum_id: key,
                variant: 0,
            },
            index: 0,
        },
        ValueType::Int,
    );
    let mut answer = exported(function(
        2,
        "answer",
        vec![],
        ValueType::Int,
        projected_value,
    ));
    answer.body.as_mut().unwrap().statements.splice(
        0..0,
        [
            ir::Statement::Pattern {
                pattern: ir::Pattern {
                    ty: marker.ty,
                    span: span(),
                    kind: ir::PatternKind::Bind(marker),
                },
                value: integer(0),
            },
            ir::Statement::Pattern {
                pattern: ir::Pattern {
                    ty: result.ty,
                    span: span(),
                    kind: ir::PatternKind::Bind(result),
                },
                value: dbg,
            },
        ],
    );
    let input = ir::Program {
        core,
        registry,
        functions: vec![read, show_key, answer],
    };
    execute_with_harness(
        input.clone(),
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int reads,logs;\nint32_t read_once(void){++reads;return 42;}\nvoid emela_board_debug(const char*text,uint16_t length){const char expected[]=\"Outer(key: custom, inner: Inner(text: \\\"x\\\"))\";if(length!=sizeof expected-1||memcmp(text,expected,length))exit(8);++logs;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;exit(9);}\nint main(void){return answer()==42 && reads==1 && logs==1 ? 0:1;}\n",
    );
    let mut invalid = input;
    invalid.functions.push(function(
        3,
        "requires_show",
        vec![],
        ValueType::String,
        expression(
            ir::ExprKind::TraitCall {
                trait_id: show,
                method: "show".into(),
                self_type: outer_ty,
                args: vec![outer_value],
                order: vec![0],
                method_type_arguments: vec![],
                nested_show: false,
            },
            ValueType::String,
        ),
    ));
    assert!(
        specialize(invalid).unwrap_err()[0]
            .message
            .contains("does not implement")
    );
}
#[test]
fn dbg_of_a_diverging_expression_does_not_require_or_evaluate_show() {
    let core = b::Definitions::default();
    let mut stop = function(0, "stop", vec![], ValueType::Never, integer(0));
    stop.body = None;
    stop.external = Some("stop".into());
    let dbg = expression(
        ir::ExprKind::Builtin {
            op: b::Builtin::Dbg,
            args: vec![call(0, vec![], vec![], ValueType::Never)],
        },
        ValueType::Never,
    );
    let answer = exported(function(1, "answer", vec![], ValueType::Int, dbg));
    execute_with_harness(
        ir::Program {
            core,
            registry: Default::default(),
            functions: vec![stop, answer],
        },
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void stop(void){exit(0);}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;exit(9);}\nint main(void){(void)answer();return 1;}\n",
    );
}

#[path = "tests_generic_methods.rs"]
mod generic_methods;
