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
fn any_boxes_retain_managed_lists_and_release_their_payloads() {
    let mut p = program(vec![]);
    let integers = p.core.types.intern(Type::List(TypeId::INT));
    let mut f = simple();
    let mut statements = vec![];
    let list = assign(
        &mut f,
        &mut statements,
        integers,
        Value::List {
            elems: vec![Const::Int(17).into()],
            tail: None,
        },
    );
    let boxed = assign(&mut f, &mut statements, TypeId::ANY, Value::Coerce(list));
    let unboxed = assign(&mut f, &mut statements, integers, Value::Coerce(boxed));
    let head = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::ListHead(unboxed),
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(head.into())),
    };
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute_repeatedly(
        output,
        "for (int i = 0; i < 10000; ++i) { assert(answer() == 17); assert(test_heap_empty()); }",
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

fn assert_int(f: &mut Function, statements: &mut Vec<StmtId>, value: VarId, expected: i32) {
    let condition = assign(
        f,
        statements,
        TypeId::BOOL,
        Value::Prim {
            op: PrimOp::Eq,
            ty: PrimTy::I32,
            args: vec![value.into(), Const::Int(expected).into()],
        },
    );
    let yes = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Yield(None),
    });
    let no = f.seqs.alloc(Seq {
        stmts: vec![],
        term: Term::Defect(Const::String("memory result mismatch".into()).into()),
    });
    statements.push(f.stmts.alloc(Stmt::If {
        dst: None,
        cond: condition.into(),
        then: yes,
        else_: no,
    }));
}

#[test]
fn bytes_optional_results_concat_and_unicode_lists_preserve_contents() {
    let mut p = program(vec![]);
    let parameter = p.core.types.intern(Type::Param(0));
    let option = p.core.enums.alloc(EnumDef {
        name: "Option".into(),
        params: 1,
        error: false,
        external: false,
        variants: vec![
            VariantDef {
                name: "Some".into(),
                fields: VariantFields::Positional(1),
                tys: vec![parameter],
            },
            VariantDef {
                name: "None".into(),
                fields: VariantFields::Unit,
                tys: vec![],
            },
        ],
    });
    let optional_bytes = p.core.types.intern(Type::Enum(option, vec![TypeId::BYTES]));
    let optional_int = p.core.types.intern(Type::Enum(option, vec![TypeId::INT]));
    let integers = p.core.types.intern(Type::List(TypeId::INT));
    let strings = p.core.types.intern(Type::List(TypeId::STRING));
    let byte_lists = p.core.types.intern(Type::List(TypeId::BYTES));
    let some = CtorRef {
        enum_id: option,
        variant: 0,
    };
    let mut f = simple();
    let mut statements = vec![];
    let input = assign(
        &mut f,
        &mut statements,
        integers,
        Value::List {
            elems: vec![
                Const::Int(0).into(),
                Const::Int(255).into(),
                Const::Int(65).into(),
            ],
            tail: None,
        },
    );
    let wrapped = builtin(
        &mut f,
        &mut statements,
        optional_bytes,
        Builtin::BytesFromList,
        vec![input.into()],
    );
    let bytes = assign(
        &mut f,
        &mut statements,
        TypeId::BYTES,
        Value::Field {
            base: wrapped,
            ctor: some,
            index: 0,
        },
    );
    let length = builtin(
        &mut f,
        &mut statements,
        TypeId::INT,
        Builtin::BytesSize,
        vec![bytes.into()],
    );
    assert_int(&mut f, &mut statements, length, 3);
    let wrapped = builtin(
        &mut f,
        &mut statements,
        optional_int,
        Builtin::BytesGet,
        vec![bytes.into(), Const::Int(1).into()],
    );
    let byte = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::Field {
            base: wrapped,
            ctor: some,
            index: 0,
        },
    );
    assert_int(&mut f, &mut statements, byte, 255);
    let pair = assign(
        &mut f,
        &mut statements,
        byte_lists,
        Value::List {
            elems: vec![bytes.into(), bytes.into()],
            tail: None,
        },
    );
    let concat = builtin(
        &mut f,
        &mut statements,
        TypeId::BYTES,
        Builtin::BytesConcat,
        vec![pair.into()],
    );
    let size = builtin(
        &mut f,
        &mut statements,
        TypeId::INT,
        Builtin::BytesSize,
        vec![concat.into()],
    );
    assert_int(&mut f, &mut statements, size, 6);
    let input = assign(
        &mut f,
        &mut statements,
        integers,
        Value::List {
            elems: vec![Const::Int(-1).into()],
            tail: None,
        },
    );
    let invalid = builtin(
        &mut f,
        &mut statements,
        optional_bytes,
        Builtin::BytesFromList,
        vec![input.into()],
    );
    let tag = assign(&mut f, &mut statements, TypeId::INT, Value::Tag(invalid));
    assert_int(&mut f, &mut statements, tag, 1);
    let invalid = builtin(
        &mut f,
        &mut statements,
        optional_int,
        Builtin::BytesGet,
        vec![bytes.into(), Const::Int(3).into()],
    );
    let tag = assign(&mut f, &mut statements, TypeId::INT, Value::Tag(invalid));
    assert_int(&mut f, &mut statements, tag, 1);
    let parts = builtin(
        &mut f,
        &mut statements,
        strings,
        Builtin::StringSplit,
        vec![
            Const::String("a::b".into()).into(),
            Const::String("::".into()).into(),
        ],
    );
    let joined = builtin(
        &mut f,
        &mut statements,
        TypeId::STRING,
        Builtin::StringJoin,
        vec![parts.into(), Const::String(":".into()).into()],
    );
    let size = builtin(
        &mut f,
        &mut statements,
        TypeId::INT,
        Builtin::StringByteSize,
        vec![joined.into()],
    );
    assert_int(&mut f, &mut statements, size, 3);
    let points = builtin(
        &mut f,
        &mut statements,
        integers,
        Builtin::StringCodePoints,
        vec![Const::String("é".into()).into()],
    );
    let first = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::ListHead(points),
    );
    assert_int(&mut f, &mut statements, first, 233);
    f.seqs[f.body.unwrap()].stmts = statements;
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute_repeatedly(
        output,
        "for(int i=0;i<1000;++i) { assert(answer()==42); assert(test_heap_empty()); }",
    );
}

#[test]
fn discarded_list_construction_keeps_fixed_heap_exhaustion() {
    let mut p = program(vec![]);
    p.core.types.intern(Type::List(TypeId::INT));
    let mut f = simple();
    let allocation = binding(
        &mut f,
        None,
        Value::List {
            elems: (0..256).map(|n| Const::Int(n).into()).collect(),
            tail: None,
        },
    );
    f.seqs[f.body.unwrap()].stmts.push(allocation);
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  (void)m; (void)n; exit(k==EMELA_DEFECT_HEAP_EXHAUSTED ? 0 : 1);
}

int main(void) { answer(); return 2; }
"#,
        0,
    );
}

fn generic_enum(p: &mut Program, name: &str, params: usize) -> EnumId {
    p.core.enums.alloc(EnumDef {
        name: name.into(),
        params,
        error: false,
        external: false,
        variants: vec![VariantDef {
            name: "End".into(),
            fields: VariantFields::Unit,
            tys: vec![],
        }],
    })
}
fn next_variant(p: &mut Program, id: EnumId, field: TypeId) {
    p.core.enums[id].variants.push(VariantDef {
        name: "Next".into(),
        fields: VariantFields::Positional(1),
        tys: vec![field],
    });
}
fn enum_tag_function(p: &mut Program, id: EnumId, args: Vec<TypeId>) {
    let ty = p.core.types.intern(Type::Enum(id, args));
    let mut f = simple();
    let mut statements = vec![];
    let value = assign(
        &mut f,
        &mut statements,
        ty,
        Value::Make {
            ctor: CtorRef {
                enum_id: id,
                variant: 0,
            },
            args: vec![],
        },
    );
    let tag = assign(&mut f, &mut statements, TypeId::INT, Value::Tag(value));
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(tag.into())),
    };
    p.functions.alloc(f);
}

#[test]
fn unused_growing_recursive_payloads_remain_opaque_without_expansion() {
    // Mirrors the positive fixed-old emit.rs growing-enum fixture, with C execution added.
    let mut p = program(vec![]);
    let id = generic_enum(&mut p, "Growing", 1);
    let parameter = p.core.types.intern(Type::Param(0));
    let nested = p.core.types.intern(Type::List(parameter));
    let recursive = p.core.types.intern(Type::Enum(id, vec![nested]));
    next_variant(&mut p, id, recursive);
    enum_tag_function(&mut p, id, vec![TypeId::INT]);
    let lowered = lower_program(p.clone()).unwrap();
    assert!(lowered.lowered.core.expanding_enums.contains(&id));
    assert!(lowered.lowered.core.types.len() < 32);
    let output =
        crate::backend::compile_program(p.clone(), &Options::default().with_heap_bytes(1024))
            .unwrap();
    execute_repeatedly(
        output,
        "for(int i=0;i<1000;++i) { assert(answer()==0); assert(test_heap_empty()); }",
    );

    // The legacy fixture also exports the aggregate itself, including its opaque Next field.
    let ty = p.core.types.intern(Type::Enum(id, vec![TypeId::INT]));
    p.functions = Default::default();
    let mut f = simple();
    f.ret = ty;
    let mut statements = vec![];
    let value = assign(
        &mut f,
        &mut statements,
        ty,
        Value::Make {
            ctor: CtorRef {
                enum_id: id,
                variant: 0,
            },
            args: vec![],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(value.into())),
    };
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(1024)).unwrap();
    execute_repeatedly(
        output,
        "for(int i=0;i<1000;++i) { assert(answer().tag==0); assert(test_heap_empty()); }",
    );
}

#[test]
fn parameter_flow_distinguishes_mutual_growth_from_permutation_and_reset() {
    let mut p = program(vec![]);
    let first = generic_enum(&mut p, "First", 1);
    let second = generic_enum(&mut p, "Second", 1);
    let parameter = p.core.types.intern(Type::Param(0));
    let list = p.core.types.intern(Type::List(parameter));
    let to_second = p.core.types.intern(Type::Enum(second, vec![list]));
    let to_first = p.core.types.intern(Type::Enum(first, vec![parameter]));
    next_variant(&mut p, first, to_second);
    next_variant(&mut p, second, to_first);
    enum_tag_function(&mut p, first, vec![TypeId::INT]);
    let lowered = lower_program(p.clone()).unwrap();
    assert_eq!(
        lowered.lowered.core.expanding_enums,
        HashSet::from([first, second])
    );
    let reset = p.core.types.intern(Type::Enum(first, vec![TypeId::INT]));
    p.core.enums[second].variants[1].tys = vec![reset];
    let lowered = lower_program(p).unwrap();
    assert!(lowered.lowered.core.expanding_enums.is_empty());

    let mut p = program(vec![]);
    let id = generic_enum(&mut p, "Permuted", 2);
    let a = p.core.types.intern(Type::Param(0));
    let b = p.core.types.intern(Type::Param(1));
    let recursive = p.core.types.intern(Type::Enum(id, vec![b, a]));
    next_variant(&mut p, id, recursive);
    enum_tag_function(&mut p, id, vec![TypeId::INT, TypeId::STRING]);
    let lowered = lower_program(p).unwrap();
    assert!(lowered.lowered.core.expanding_enums.is_empty());
}

#[test]
fn recursive_enum_edges_inside_tuples_are_boxed_and_released() {
    let mut p = program(vec![]);
    let id = generic_enum(&mut p, "TupleChain", 0);
    let chain = p.core.types.intern(Type::Enum(id, vec![]));
    let tuple = p.core.types.intern(Type::Tuple(vec![TypeId::INT, chain]));
    next_variant(&mut p, id, tuple);
    let mut f = simple();
    let mut statements = vec![];
    let end = assign(
        &mut f,
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
    let fields = assign(
        &mut f,
        &mut statements,
        tuple,
        Value::Tuple(vec![Const::Int(7).into(), end.into()]),
    );
    let node = assign(
        &mut f,
        &mut statements,
        chain,
        Value::Make {
            ctor: CtorRef {
                enum_id: id,
                variant: 1,
            },
            args: vec![fields.into()],
        },
    );
    let fields = assign(
        &mut f,
        &mut statements,
        tuple,
        Value::Field {
            base: node,
            ctor: CtorRef {
                enum_id: id,
                variant: 1,
            },
            index: 0,
        },
    );
    let head = assign(
        &mut f,
        &mut statements,
        TypeId::INT,
        Value::TupleGet {
            base: fields,
            index: 0,
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(head.into())),
    };
    p.functions.alloc(f);
    let output =
        crate::backend::compile_program(p, &Options::default().with_heap_bytes(4096)).unwrap();
    execute_repeatedly(
        output,
        "for(int i=0;i<1000;++i) { assert(answer()==7); assert(test_heap_empty()); }",
    );
}
