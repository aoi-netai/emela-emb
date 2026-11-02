use super::*;

fn assign(f: &mut Function, statements: &mut Vec<StmtId>, ty: TypeId, value: Value) -> VarId {
    let var = match ty {
        TypeId::INT | TypeId::INT64 | TypeId::FLOAT | TypeId::BOOL | TypeId::UNIT => {
            variable(f, ty)
        }
        _ => aggregate_variable(f, ty),
    };
    statements.push(binding(f, Some(var), value));
    var
}

fn builtin(
    f: &mut Function,
    statements: &mut Vec<StmtId>,
    ty: TypeId,
    op: Builtin,
    args: Vec<Operand>,
) -> VarId {
    assign(f, statements, ty, Value::Builtin { op, args })
}

fn execute_repeatedly(mut output: Output, calls: &str) {
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    assert!(source.contains("emela_heap_storage"));
    assert!(!source.contains("malloc("));
    source.push_str(
        r#"
int test_heap_empty(void) {
  if (!emela_heap_initialized) return 1;
  for (unsigned char *p = emela_heap_storage; p < emela_heap_end();) {
    emela_heap_block *b = (emela_heap_block *)(void *)p;
    if (!b->is_free) return 0;
    p += b->size;
  }
  return 1;
}
"#,
    );
    execute(
        &output,
        &format!(
            r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *s, uint16_t n) {{
    fprintf(stderr, "defect %d: %.*s\n", k, n, s); abort();
}}
int main(void) {{ {calls} return 0; }}
"#
        ),
        0,
    );
}

#[test]
fn list_string_bytes_and_any_release_owned_values_after_repeated_calls() {
    let mut p = program(vec![]);
    let strings = p.core.types.intern(Type::List(TypeId::STRING));
    let integers = p.core.types.intern(Type::List(TypeId::INT));
    let mut f = simple();
    let input = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(input);
    let mut statements = vec![];
    let boxed = assign(&mut f, &mut statements, TypeId::ANY, Value::Coerce(input));
    let unboxed = assign(
        &mut f,
        &mut statements,
        TypeId::STRING,
        Value::Coerce(boxed),
    );
    let chars = builtin(
        &mut f,
        &mut statements,
        strings,
        Builtin::StringChars,
        vec![unboxed.into()],
    );
    let joined = builtin(
        &mut f,
        &mut statements,
        TypeId::STRING,
        Builtin::StringJoin,
        vec![chars.into(), Const::String(String::new()).into()],
    );
    let bytes = builtin(
        &mut f,
        &mut statements,
        TypeId::BYTES,
        Builtin::BytesEncodeUtf8,
        vec![joined.into()],
    );
    let list = builtin(
        &mut f,
        &mut statements,
        integers,
        Builtin::BytesToList,
        vec![bytes.into()],
    );
    let tail = assign(&mut f, &mut statements, integers, Value::ListTail(list));
    let byte = assign(&mut f, &mut statements, TypeId::INT, Value::ListHead(tail));
    let slice = builtin(
        &mut f,
        &mut statements,
        TypeId::BYTES,
        Builtin::BytesSlice,
        vec![bytes.into(), Const::Int(1).into(), Const::Int(2).into()],
    );
    let decoded = builtin(
        &mut f,
        &mut statements,
        TypeId::STRING,
        Builtin::BytesDecodeValidUtf8,
        vec![slice.into()],
    );
    let length = builtin(
        &mut f,
        &mut statements,
        TypeId::INT,
        Builtin::StringByteSize,
        vec![decoded.into()],
    );
    let result = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::Prim {
            op: PrimOp::Add,
            ty: PrimTy::I32,
            args: vec![byte.into(), length.into()],
        },
    );
    // Even discarded allocation must execute, then release its managed String elements.
    statements.push(binding(
        &mut f,
        None,
        Value::List {
            elems: vec![input.into()],
            tail: None,
        },
    ));
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute_repeatedly(
        output,
        r#"
char text[] = {'A', 0, (char)0xf0, (char)0x9f, (char)0x98, (char)0x80};
for (int i = 0; i < 10000; ++i) {
  assert(answer((EmelaString){text, sizeof(text), NULL}) == 1);
  assert(test_heap_empty());
}
"#,
    );
}

#[test]
fn recursive_enum_tail_construction_and_deep_release_use_fixed_heap() {
    let mut p = program(vec![]);
    let id = p.core.enums.alloc(EnumDef {
        name: "Chain".into(),
        params: 0,
        variants: vec![],
        error: false,
        external: false,
    });
    let chain = p.core.types.intern(Type::Enum(id, vec![]));
    p.core.enums[id].variants = vec![
        VariantDef {
            name: "End".into(),
            fields: VariantFields::Unit,
            tys: vec![],
        },
        VariantDef {
            name: "Link".into(),
            fields: VariantFields::Positional(2),
            tys: vec![TypeId::INT, chain],
        },
    ];
    let mut build = Function::new("build");
    build.ret = chain;
    let count = variable(&mut build, TypeId::INT);
    let acc = aggregate_variable(&mut build, chain);
    build.params = vec![count, acc];
    let mut statements = vec![];
    let done = assign(
        &mut build,
        &mut statements,
        TypeId::BOOL,
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(0).into()],
        },
    );
    let yes = build.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Return(Some(acc.into())),
    });
    let mut next_statements = vec![];
    let node = assign(
        &mut build,
        &mut next_statements,
        chain,
        Value::Make {
            ctor: CtorRef {
                enum_id: id,
                variant: 1,
            },
            args: vec![count.into(), acc.into()],
        },
    );
    let next = assign(
        &mut build,
        &mut next_statements,
        TypeId::INT,
        Value::Prim {
            op: PrimOp::Sub,
            ty: PrimTy::I32,
            args: vec![count.into(), Const::Int(1).into()],
        },
    );
    let no = build.seqs.alloc(Seq {
        stmts: next_statements,
        term: Term::Recur(vec![next.into(), node.into()]),
    });
    statements.push(build.stmts.alloc(Stmt::If {
        dst: None,
        cond: done.into(),
        then: yes,
        else_: no,
    }));
    build.body = Some(build.seqs.alloc(Seq {
        stmts: statements,
        term: Term::Unreachable,
    }));
    let build_id = p.functions.alloc(build);
    let mut answer = simple();
    let mut statements = vec![];
    let end = assign(
        &mut answer,
        &mut statements,
        chain,
        Value::Make {
            ctor: CtorRef {
                enum_id: id,
                variant: 0,
            },
            args: vec![],
        },
    );
    let node = assign(
        &mut answer,
        &mut statements,
        chain,
        Value::Call {
            callee: Callee::Direct(build_id),
            args: vec![Const::Int(20000).into(), end.into()],
            suspends: false,
            on_fail: None,
        },
    );
    let head = assign(
        &mut answer,
        &mut statements,
        TypeId::INT,
        Value::Field {
            base: node,
            ctor: CtorRef {
                enum_id: id,
                variant: 1,
            },
            index: 0,
        },
    );
    answer.seqs[answer.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(head.into())),
    };
    p.functions.alloc(answer);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(2 * 1024 * 1024))
            .unwrap();
    execute_repeatedly(
        output,
        "for(int i=0;i<5;++i) { assert(answer()==1); assert(test_heap_empty()); }",
    );
}

#[test]
fn rejects_mismatched_list_elements_and_unrelated_data_coercions() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::INT));
    let mut f = simple();
    let mut statements = vec![];
    assign(
        &mut f,
        &mut statements,
        list,
        Value::List {
            elems: vec![Const::Bool(true).into()],
            tail: None,
        },
    );
    f.seqs[f.body.unwrap()].stmts = statements;
    p.functions.alloc(f);
    assert!(
        lower_program(p).unwrap_err()[0]
            .message
            .contains("type mismatch")
    );
    let mut f = simple();
    let mut statements = vec![];
    let value = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::Const(Const::Int(1)),
    );
    assign(&mut f, &mut statements, TypeId::BOOL, Value::Coerce(value));
    f.seqs[f.body.unwrap()].stmts = statements;
    assert!(
        lower_program(program(vec![f])).unwrap_err()[0]
            .message
            .contains("coercion")
    );
}
