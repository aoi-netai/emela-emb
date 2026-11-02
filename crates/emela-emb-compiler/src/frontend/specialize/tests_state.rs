use super::*;

fn fn_type(core: &mut b::Definitions, params: Vec<b::TypeId>, ret: b::TypeId) -> b::TypeId {
    core.types.intern(b::Type::Fn(b::FnType {
        params,
        ret,
        fails: false,
        suspends: false,
        ev: false,
        sets: None,
    }))
}
fn reference(local: &ir::Local) -> ir::Expr {
    expression(ir::ExprKind::Local(local.id), local.ty)
}
fn bind(local: ir::Local, value: ir::Expr) -> ir::Statement {
    ir::Statement::Pattern {
        pattern: ir::Pattern {
            ty: local.ty,
            kind: ir::PatternKind::Bind(local),
            span: span(),
        },
        value,
    }
}
fn state_program() -> ir::Program {
    let mut core = b::Definitions::default();
    let update_ty = fn_type(&mut core, vec![b::TypeId::INT], b::TypeId::INT);
    let reader_ty = fn_type(&mut core, vec![], b::TypeId::INT);
    let effect = core.effects.alloc(b::EffectDef {
        name: "Std.State.State[Int]".into(),
        decl: "Std.State.State".into(),
        args: vec![b::TypeId::INT],
        label_of: None,
        ops: vec![
            b::OpDef {
                name: "get".into(),
                suspend: false,
                type_params: 0,
                params: vec![],
                ret: b::TypeId::INT,
                fails: false,
            },
            b::OpDef {
                name: "set".into(),
                suspend: false,
                type_params: 0,
                params: vec![b::TypeId::INT],
                ret: b::TypeId::UNIT,
                fails: false,
            },
            b::OpDef {
                name: "update".into(),
                suspend: false,
                type_params: 0,
                params: vec![update_ty],
                ret: b::TypeId::UNIT,
                fails: false,
            },
        ],
    });
    let handler_ty = core.types.intern(b::Type::Handler(effect));
    let empty = core.types.intern(b::Type::Set(b::SetSummary::default()));
    let effects = core.types.intern(b::Type::Set(b::SetSummary {
        params: vec![handler_ty],
        ..Default::default()
    }));
    let callback_ty = core.types.intern(b::Type::Fn(b::FnType {
        params: vec![],
        ret: reader_ty,
        fails: false,
        suspends: false,
        ev: true,
        sets: Some(b::FnSets {
            errors: empty,
            effects,
        }),
    }));
    let result_ty = frontend_type(
        core.types
            .intern(b::Type::Tuple(vec![reader_ty, b::TypeId::INT])),
    );
    let mut runtime = function(
        0,
        "State.run",
        vec![
            local(0, ValueType::Int),
            local(1, frontend_type(callback_ty)),
        ],
        result_ty,
        integer(0),
    );
    runtime.body = None;
    runtime.intrinsic = Some(crate::sources::Intrinsic::StateRun);
    let capability = local(0, frontend_type(handler_ty));
    let value = local(1, ValueType::Int);
    let increment = expression(
        ir::ExprKind::Lambda {
            params: vec![value.clone()],
            captures: vec![],
            body: block(expression(
                ir::ExprKind::Binary {
                    op: Binary::Add,
                    left: Box::new(reference(&value)),
                    right: Box::new(integer(1)),
                },
                ValueType::Int,
            )),
        },
        frontend_type(update_ty),
    );
    let update = expression(
        ir::ExprKind::OpCall {
            handler: Box::new(reference(&capability)),
            effect,
            op: 2,
            args: vec![increment],
            order: vec![0],
        },
        ValueType::Unit,
    );
    let get = expression(
        ir::ExprKind::OpCall {
            handler: Box::new(reference(&capability)),
            effect,
            op: 0,
            args: vec![],
            order: vec![],
        },
        ValueType::Int,
    );
    let reader = expression(
        ir::ExprKind::Lambda {
            params: vec![],
            captures: vec![capability.id],
            body: ir::Block {
                statements: vec![ir::Statement::Expr(update), ir::Statement::Expr(get)],
                ty: ValueType::Int,
                span: span(),
            },
        },
        frontend_type(reader_ty),
    );
    let callback = expression(
        ir::ExprKind::Lambda {
            params: vec![],
            captures: vec![],
            body: ir::Block {
                statements: vec![
                    bind(
                        capability,
                        expression(ir::ExprKind::Use { effect }, frontend_type(handler_ty)),
                    ),
                    ir::Statement::Expr(reader),
                ],
                ty: frontend_type(reader_ty),
                span: span(),
            },
        },
        frontend_type(callback_ty),
    );
    let callback = expression(
        ir::ExprKind::Block(ir::Block {
            statements: vec![
                ir::Statement::Expr(call(2, vec![integer(1)], vec![], ValueType::Int)),
                ir::Statement::Expr(callback),
            ],
            ty: frontend_type(callback_ty),
            span: span(),
        }),
        frontend_type(callback_ty),
    );
    let initial = expression(
        ir::ExprKind::Binary {
            op: Binary::Add,
            left: Box::new(call(2, vec![integer(2)], vec![], ValueType::Int)),
            right: Box::new(integer(38)),
        },
        ValueType::Int,
    );
    let run = expression(
        ir::ExprKind::Call {
            target: ir::DefinitionId(0),
            args: vec![callback, initial],
            order: vec![1, 0],
            type_arguments: vec![],
        },
        result_ty,
    );
    let reader = local(10, frontend_type(reader_ty));
    let result_pattern = ir::Pattern {
        ty: result_ty,
        span: span(),
        kind: ir::PatternKind::Tuple(vec![
            ir::Pattern {
                ty: reader.ty,
                span: span(),
                kind: ir::PatternKind::Bind(reader.clone()),
            },
            ir::Pattern {
                ty: ValueType::Int,
                span: span(),
                kind: ir::PatternKind::Any,
            },
        ]),
    };
    let apply = || {
        expression(
            ir::ExprKind::Apply {
                callee: Box::new(reference(&reader)),
                args: vec![],
                order: vec![],
            },
            ValueType::Int,
        )
    };
    let answer = exported(function(
        1,
        "answer",
        vec![],
        ValueType::Int,
        expression(
            ir::ExprKind::Block(ir::Block {
                statements: vec![
                    ir::Statement::Pattern {
                        pattern: result_pattern,
                        value: run,
                    },
                    ir::Statement::Expr(expression(
                        ir::ExprKind::Binary {
                            op: Binary::Add,
                            left: Box::new(apply()),
                            right: Box::new(apply()),
                        },
                        ValueType::Int,
                    )),
                ],
                ty: ValueType::Int,
                span: span(),
            }),
            ValueType::Int,
        ),
    ));
    let mut mark = function(
        2,
        "mark",
        vec![local(0, ValueType::Int)],
        ValueType::Int,
        integer(0),
    );
    mark.external = Some("mark".into());
    mark.body = None;
    ir::Program {
        core,
        registry: Default::default(),
        functions: vec![runtime, answer, mark],
    }
}

#[test]
fn state_expansion_preserves_named_argument_order_and_escaped_shared_cells() {
    execute_with_options(
        state_program(),
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static int trace;
int32_t mark(int32_t n){trace=trace*10+n;return n;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){trace=0;assert(answer()==83&&trace==12&&test_heap_empty());}return 0;}
"#,
        &crate::Options::default().with_heap_bytes(8192),
    );
}

#[test]
fn state_expansion_requires_the_compiler_owned_effect_identity() {
    let mut program = state_program();
    for (_, effect) in program.core.effects.iter_mut() {
        effect.decl = "User.State".into();
    }
    let error = specialize(program).unwrap_err();
    assert!(
        error
            .iter()
            .any(|error| error.message.contains("concrete State effect"))
    );
}
