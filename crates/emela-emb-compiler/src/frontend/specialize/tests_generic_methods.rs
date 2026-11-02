use super::*;
use crate::frontend::traits::{BuiltinTrait, Impl, Method, Registry, Trait, TypeHead};

#[test]
fn generic_trait_methods_keep_impl_and_method_arguments_separate_through_defaults() {
    let mut core = b::Definitions::default();
    let p0 = core.types.intern(b::Type::Param(0));
    let p1 = core.types.intern(b::Type::Param(1));
    let wrapper = core.enums.alloc(b::EnumDef {
        name: "Wrapper".into(),
        params: 1,
        variants: vec![b::VariantDef {
            name: "Wrapper".into(),
            fields: b::VariantFields::Positional(1),
            tys: vec![p0],
        }],
        error: false,
        external: false,
    });
    let fallback = core.enums.alloc(b::EnumDef {
        name: "Fallback".into(),
        params: 0,
        variants: vec![b::VariantDef {
            name: "Fallback".into(),
            fields: b::VariantFields::Unit,
            tys: vec![],
        }],
        error: false,
        external: false,
    });
    let wrapper_generic = core.types.intern(b::Type::Enum(wrapper, vec![p0]));
    let wrapper_bool = core
        .types
        .intern(b::Type::Enum(wrapper, vec![b::TypeId::BOOL]));
    let fallback_ty = core.types.intern(b::Type::Enum(fallback, vec![]));
    let signature = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![p0, p1],
        ret: p1,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }));
    let mut registry = Registry::new();
    let eq = registry
        .add_trait(Trait {
            name: "Eq".into(),
            owner_module: 0,
            supers: vec![],
            methods: vec![],
        })
        .unwrap();
    registry.set_builtin(eq, BuiltinTrait::Eq).unwrap();
    let select = registry
        .add_trait(Trait {
            name: "Select".into(),
            owner_module: 1,
            supers: vec![],
            methods: vec![Method {
                name: "select".into(),
                signature,
                type_parameters: vec![vec![eq]],
                default_function: Some(ir::DefinitionId(1)),
            }],
        })
        .unwrap();
    for (id, parameters, methods) in [
        (
            wrapper,
            vec![vec![]],
            HashMap::from([("select".into(), ir::DefinitionId(0))]),
        ),
        (fallback, vec![], HashMap::new()),
    ] {
        registry.register_type_owner(id, 1).unwrap();
        registry
            .add_impl(
                &core,
                Impl {
                    head: TypeHead::Enum(id),
                    trait_id: select,
                    owner_module: 1,
                    type_parameters: parameters,
                    methods,
                },
            )
            .unwrap();
    }
    let empty_bounds = Default::default();
    let explicit = registry
        .resolve_generic(
            &core,
            wrapper_bool,
            select,
            "select",
            &[b::TypeId::INT],
            &empty_bounds,
        )
        .unwrap();
    assert_eq!(
        explicit.type_arguments,
        vec![b::TypeId::BOOL, b::TypeId::INT]
    );
    assert_eq!(explicit.definition, Some(ir::DefinitionId(0)));
    let from_default = registry
        .resolve_generic(
            &core,
            fallback_ty,
            select,
            "select",
            &[b::TypeId::STRING],
            &empty_bounds,
        )
        .unwrap();
    assert_eq!(
        from_default.type_arguments,
        vec![fallback_ty, b::TypeId::STRING]
    );
    assert!(from_default.from_default);
    assert!(
        registry
            .resolve(&core, wrapper_bool, select, "select", &empty_bounds)
            .is_err()
    );
    assert!(
        registry
            .resolve_generic(
                &core,
                wrapper_bool,
                select,
                "select",
                &[signature],
                &empty_bounds
            )
            .is_err()
    );
    let mut implementation = function(
        0,
        "select_impl",
        vec![
            local(0, frontend_type(wrapper_generic)),
            local(1, frontend_type(p1)),
        ],
        frontend_type(p1),
        expression(ir::ExprKind::Local(ir::LocalId(1)), frontend_type(p1)),
    );
    implementation.type_parameters = 2;
    let mut default = function(
        1,
        "select_default",
        vec![local(0, frontend_type(p0)), local(1, frontend_type(p1))],
        frontend_type(p1),
        expression(ir::ExprKind::Local(ir::LocalId(1)), frontend_type(p1)),
    );
    default.type_parameters = 2;
    let mut dispatch = function(
        2,
        "dispatch",
        vec![local(0, frontend_type(p0)), local(1, frontend_type(p1))],
        frontend_type(p1),
        expression(
            ir::ExprKind::TraitCall {
                trait_id: select,
                method: "select".into(),
                self_type: p0,
                method_type_arguments: vec![p1],
                args: vec![
                    expression(ir::ExprKind::Local(ir::LocalId(0)), frontend_type(p0)),
                    expression(ir::ExprKind::Local(ir::LocalId(1)), frontend_type(p1)),
                ],
                order: vec![0, 1],
                nested_show: false,
            },
            frontend_type(p1),
        ),
    );
    dispatch.type_parameters = 2;
    let make = |id, ty, args: Vec<ir::Expr>| {
        expression(
            ir::ExprKind::Make {
                ctor: b::CtorRef {
                    enum_id: id,
                    variant: 0,
                },
                order: (0..args.len()).collect(),
                args,
            },
            frontend_type(ty),
        )
    };
    let number = call(
        2,
        vec![
            make(
                wrapper,
                wrapper_bool,
                vec![expression(
                    ir::ExprKind::Literal(ir::Literal::Bool(true)),
                    ValueType::Bool,
                )],
            ),
            integer(42),
        ],
        vec![wrapper_bool, b::TypeId::INT],
        ValueType::Int,
    );
    let text = || {
        expression(
            ir::ExprKind::Literal(ir::Literal::String("method".into())),
            ValueType::String,
        )
    };
    let string = call(
        2,
        vec![make(fallback, fallback_ty, vec![]), text()],
        vec![fallback_ty, b::TypeId::STRING],
        ValueType::String,
    );
    let answer = exported(function(
        3,
        "answer",
        vec![],
        ValueType::Int,
        expression(
            ir::ExprKind::If {
                condition: Box::new(expression(
                    ir::ExprKind::Binary {
                        op: Binary::Eq,
                        left: Box::new(string),
                        right: Box::new(text()),
                    },
                    ValueType::Bool,
                )),
                then: block(number),
                else_: block(integer(0)),
            },
            ValueType::Int,
        ),
    ));
    let input = ir::Program {
        core,
        registry,
        functions: vec![implementation, default, dispatch, answer],
    };
    let specialized = specialize(input.clone()).unwrap();
    assert_eq!(specialized.functions.len(), 5);
    assert!(specialized.functions.iter().all(|f| f.type_parameters == 0));
    execute(input);
}
