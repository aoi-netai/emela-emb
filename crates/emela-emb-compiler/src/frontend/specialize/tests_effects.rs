use super::*;

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
    get.signature.sets.effects.push("Read".into());
    let callback = local(0, effectful);
    let mut invoke = function(
        1,
        "invoke",
        vec![callback.clone()],
        ValueType::Int,
        apply(reference(&callback)),
    );
    invoke.signature.sets.effects.push("Read".into());
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
    get.signature.sets.effects.push("Read".into());
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
