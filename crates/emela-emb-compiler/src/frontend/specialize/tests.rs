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
    std::fs::write(path.join("harness.c"),"#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n) {(void)k;(void)m;(void)n;exit(9);}\nint main(void) {return answer()==42 ? 0 : 1;}\n").unwrap();
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
