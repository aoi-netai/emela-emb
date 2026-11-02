use super::*;

#[test]
fn split_views_and_distinct_join_results_survive_a_real_suspension_with_nul_and_unicode() {
    let mut p = program(vec![]);
    let list = p.core.types.intern(Type::List(TypeId::STRING));
    let mut wait = Function::new("wait");
    wait.suspends = true;
    wait.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("wait".into()),
        }],
    });
    let wait = p.functions.alloc(wait);
    let mut f = simple();
    f.ret = TypeId::STRING;
    f.suspends = true;
    let input = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(input);
    let parts = aggregate_variable(&mut f, list);
    let before = aggregate_variable(&mut f, TypeId::STRING);
    let after = aggregate_variable(&mut f, TypeId::STRING);
    let split = binding(
        &mut f,
        Some(parts),
        Value::Builtin {
            op: Builtin::StringSplit,
            args: vec![input.into(), Const::String("|".into()).into()],
        },
    );
    let join_before = binding(
        &mut f,
        Some(before),
        Value::Builtin {
            op: Builtin::StringJoin,
            args: vec![parts.into(), Const::String("/".into()).into()],
        },
    );
    let suspend = binding(
        &mut f,
        None,
        Value::Call {
            callee: Callee::Direct(wait),
            args: vec![],
            suspends: true,
            on_fail: None,
        },
    );
    let join_after = binding(
        &mut f,
        Some(after),
        Value::Builtin {
            op: Builtin::StringJoin,
            args: vec![parts.into(), Const::String(":".into()).into()],
        },
    );
    let mut statements = vec![split, join_before, suspend, join_after];
    for (value, text) in [(before, "a\0b/β/z"), (after, "a\0b:β:z")] {
        let equal = variable(&mut f, TypeId::BOOL);
        statements.push(binding(
            &mut f,
            Some(equal),
            Value::Prim {
                op: PrimOp::Eq,
                ty: PrimTy::String,
                args: vec![value.into(), Const::String(text.into()).into()],
            },
        ));
        let yes = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Yield(None),
        });
        let no = f.seqs.alloc(Seq {
            stmts: vec![],
            term: Term::Defect(
                Const::String("join result changed across suspension".into()).into(),
            ),
        });
        statements.push(f.stmts.alloc(Stmt::If {
            dst: None,
            cond: equal.into(),
            then: yes,
            else_: no,
        }));
    }
    f.seqs[f.body.unwrap()] = Seq {
        stmts: statements,
        term: Term::Return(Some(after.into())),
    };
    p.functions.alloc(f);
    let mut output = crate::backend::compile_program(
        p,
        &Options::default()
            .with_heap_bytes(8192)
            .with_frame_bytes(1024)
            .with_string_capacity(8)
            .with_task_adapter("wait", "board_wait_start"),
    )
    .unwrap();
    audit_owned_heap(&mut output);
    execute(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);static EmelaTask pending;static int started;
void board_wait_start(EmelaTask task){pending=task;started++;}
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){char bytes[]={'a',0,'b','|',(char)0xce,(char)0xb2,'|','z'};const char expected[]={'a',0,'b',':',(char)0xce,(char)0xb2,':','z'};EmelaString result={0};started=0;EmelaFiber f=answer_start((EmelaString){bytes,8,NULL},&result);memset(bytes,'x',sizeof(bytes));while(emela_scheduler_run(1)){}if(started!=1||result.length||emela_fiber_done(f))return 1;if(!emela_task_complete_unit(pending)||emela_task_complete_unit(pending))return 2;while(!emela_fiber_done(f))if(!emela_scheduler_run(1))return 3;emela_fiber_release(f);if(!result.owner||result.length!=8||memcmp(result.data,expected,8))return 4;emela_embedded_release_string(&result);if(!test_heap_empty()||emela_task_complete_unit(pending))return 5;}return 0;}
"#,
        0,
    );
}

#[test]
fn constant_numeric_and_string_values_fold_and_constant_float_append_keeps_its_exact_bound() {
    let mut p = program(vec![]);
    let mut round = Function::new("round");
    round.exported = true;
    round.export_symbol = Some("rounded".into());
    round.constant = true;
    round.ret = TypeId::INT;
    let n = variable(&mut round, TypeId::INT);
    let calc = binding(
        &mut round,
        Some(n),
        Value::Builtin {
            op: Builtin::FloatRound,
            args: vec![Const::Float(-2.5).into()],
        },
    );
    round.body = Some(round.seqs.alloc(Seq {
        stmts: vec![calc],
        term: Term::Return(Some(n.into())),
    }));
    p.functions.alloc(round);
    let mut joined = Function::new("joined");
    joined.exported = true;
    joined.export_symbol = Some("joined".into());
    joined.constant = true;
    joined.ret = TypeId::STRING;
    let s = aggregate_variable(&mut joined, TypeId::STRING);
    let join = binding(
        &mut joined,
        Some(s),
        Value::Builtin {
            op: Builtin::StringAppend,
            args: vec![
                Const::String("a".into()).into(),
                Const::String("é".into()).into(),
            ],
        },
    );
    joined.body = Some(joined.seqs.alloc(Seq {
        stmts: vec![join],
        term: Term::Return(Some(s.into())),
    }));
    p.functions.alloc(joined);
    let output = crate::backend::compile_program(p, &Options::default()).unwrap();
    assert!(!output.c_source.contains("emela_emb_string_append("));
    execute(
        &output,
        "#include <string.h>\n#include \"emela_embedded.h\"\nint main(void){EmelaString s=joined();return rounded()!=-3||s.length!=3||memcmp(s.data,\"a\\xc3\\xa9\",3);}\n",
        0,
    );
    let mut f = simple();
    let input = variable(&mut f, TypeId::INT);
    f.params.push(input);
    let float_text = aggregate_variable(&mut f, TypeId::STRING);
    let integer_text = aggregate_variable(&mut f, TypeId::STRING);
    let combined = aggregate_variable(&mut f, TypeId::STRING);
    let length = variable(&mut f, TypeId::INT);
    let a = binding(
        &mut f,
        Some(float_text),
        Value::Builtin {
            op: Builtin::FloatToString,
            args: vec![Const::Float(1.0e-6).into()],
        },
    );
    let b = binding(
        &mut f,
        Some(integer_text),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![input.into()],
        },
    );
    let c = binding(
        &mut f,
        Some(combined),
        Value::Builtin {
            op: Builtin::StringAppend,
            args: vec![float_text.into(), integer_text.into()],
        },
    );
    let d = binding(
        &mut f,
        Some(length),
        Value::Builtin {
            op: Builtin::StringByteSize,
            args: vec![combined.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![a, b, c, d],
        term: Term::Return(Some(length.into())),
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    assert!(output.c_source.contains("UINT16_C(19)"));
    execute(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){return answer(42)!=10;}\n",
        0,
    );
}

#[test]
fn selected_entry_supplies_dynamic_string_buffers_at_minimum_and_maximum_capacities() {
    let mut p = program(vec![]);
    let mut read = Function::new("read_number");
    read.ret = TypeId::INT;
    read.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global("read_number".into()),
        }],
    });
    let read = p.functions.alloc(read);
    let mut f = Function::new("main");
    f.exported = true;
    f.export_symbol = Some("text".into());
    f.ret = TypeId::STRING;
    let n = variable(&mut f, TypeId::INT);
    let text = aggregate_variable(&mut f, TypeId::STRING);
    let call = binding(
        &mut f,
        Some(n),
        Value::Call {
            callee: Callee::Direct(read),
            args: vec![],
            suspends: false,
            on_fail: None,
        },
    );
    let format = binding(
        &mut f,
        Some(text),
        Value::Builtin {
            op: Builtin::IntToString,
            args: vec![n.into()],
        },
    );
    f.body = Some(f.seqs.alloc(Seq {
        stmts: vec![call, format],
        term: Term::Return(Some(text.into())),
    }));
    p.functions.alloc(f);
    for capacity in [2, 64, 512] {
        let output = crate::backend::compile_program(
            p.clone(),
            &Options::default().with_string_capacity(capacity),
        )
        .unwrap();
        execute(
            &output,
            "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nvoid emela_platform_start(void (*program)(void)){program();}static int calls;int32_t read_number(void){calls++;return 42;}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)emela_embedded_start();if(calls!=1000)return 1;char buffer[2];EmelaString s=text(buffer,2);return s.data!=buffer||s.owner||s.length!=2||memcmp(s.data,\"42\",2)||calls!=1001;}\n",
            0,
        );
    }
}
