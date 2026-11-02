use super::*;

#[test]
fn debug_print_passes_the_original_string_view_to_the_board_hook() {
    let mut f = simple();
    f.ret = TypeId::UNIT;
    let text = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(text);
    let print = binding(
        &mut f,
        None,
        Value::Runtime {
            op: RuntimeOp::DebugPrint,
            args: vec![text.into()],
        },
    );
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![print],
        term: Term::Return(None),
    };
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    let board = &output
        .files
        .iter()
        .find(|file| file.path == "emela_board.h")
        .unwrap()
        .contents;
    assert!(board.contains("void emela_board_debug(const char *bytes, uint16_t length);"));
    assert!(!board.contains("emela_board_defect"));
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nstatic int calls;\nvoid emela_board_debug(const char *p,uint16_t n){if(n!=3||memcmp(p,\"abc\",3))exit(1);calls++;}\nint main(void){for(int i=0;i<1000;i++)answer((EmelaString){\"xabc\"+1,3,NULL});return calls==1000?0:2;}\n",
        0,
    );
}

#[test]
fn unreachable_terms_raise_the_native_or_scheduler_defect_when_reached() {
    for suspends in [false, true] {
        let mut f = simple();
        f.ret = TypeId::NEVER;
        f.suspends = suspends;
        f.seqs[f.body.unwrap()].term = Term::Unreachable;
        let output =
            crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
        let (message, call) = if suspends {
            (
                "invalid scheduler state",
                "(void)answer_start();while(emela_scheduler_run(1)){}",
            )
        } else {
            ("unreachable code", "answer();")
        };
        let harness = format!(
            "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nuint32_t emela_board_event_lock(void){{return 0;}}\nvoid emela_board_event_unlock(uint32_t n){{(void)n;}}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{exit(k==EMELA_DEFECT_ASSERT&&n=={}&&!memcmp(m,\"{message}\",n)?0:2);}}\nint main(void){{{call}return 1;}}\n",
            message.len()
        );
        execute(&output, &harness, 0);
    }
}

#[test]
fn discarded_integer_division_and_remainder_keep_defects_in_native_and_suspend_callers() {
    for suspends in [false, true] {
        for ty in [PrimTy::I32, PrimTy::I64] {
            for op in [PrimOp::Div, PrimOp::Rem] {
                let mut f = simple();
                f.ret = TypeId::UNIT;
                f.suspends = suspends;
                let args = if ty == PrimTy::I32 {
                    vec![Const::Int(7).into(), Const::Int(0).into()]
                } else {
                    vec![Const::Int64(7).into(), Const::Int64(0).into()]
                };
                let operation = binding(&mut f, None, Value::Prim { op, ty, args });
                f.seqs[f.body.unwrap()] = Seq {
                    stmts: vec![operation],
                    term: Term::Return(None),
                };
                let output =
                    crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
                let call = if suspends {
                    "(void)answer_start();while(emela_scheduler_run(1)){}"
                } else {
                    "answer();"
                };
                let harness = format!(
                    "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\nuint32_t emela_board_event_lock(void){{return 0;}}\nvoid emela_board_event_unlock(uint32_t n){{(void)n;}}\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{(void)m;(void)n;exit(k==EMELA_DEFECT_DIVISION_BY_ZERO?0:2);}}\nint main(void){{{call}return 1;}}\n"
                );
                execute(&output, &harness, 0);
            }
        }
    }
}

#[test]
fn derived_bytes_equality_and_display_keep_unsigned_values_at_the_c_boundary() {
    let mut p = program(vec![]);
    let mut eq = Function::new("bytes_eq");
    eq.ret = TypeId::BOOL;
    eq.derivation = Some(crate::DerivedOperation::Eq);
    eq.external = Some(External { bindings: vec![] });
    let a = aggregate_variable(&mut eq, TypeId::BYTES);
    let b = aggregate_variable(&mut eq, TypeId::BYTES);
    eq.params = vec![a, b];
    let eq = p.functions.alloc(eq);
    let mut show = Function::new("bytes_show");
    show.ret = TypeId::STRING;
    show.derivation = Some(crate::DerivedOperation::Show);
    show.external = Some(External { bindings: vec![] });
    let value = aggregate_variable(&mut show, TypeId::BYTES);
    show.params.push(value);
    let show = p.functions.alloc(show);
    for (symbol, target, ret, arity) in [
        ("equal", eq, TypeId::BOOL, 2),
        ("display", show, TypeId::STRING, 1),
    ] {
        let mut f = Function::new(symbol);
        f.exported = true;
        f.export_symbol = Some(symbol.into());
        f.ret = ret;
        let args = (0..arity)
            .map(|_| {
                let v = aggregate_variable(&mut f, TypeId::BYTES);
                f.params.push(v);
                v.into()
            })
            .collect();
        let result = if ret == TypeId::BOOL {
            variable(&mut f, ret)
        } else {
            aggregate_variable(&mut f, ret)
        };
        let call = binding(
            &mut f,
            Some(result),
            Value::Call {
                callee: Callee::Direct(target),
                args,
                suspends: false,
                on_fail: None,
            },
        );
        f.body = Some(f.seqs.alloc(Seq {
            stmts: vec![call],
            term: Term::Return(Some(result.into())),
        }));
        p.functions.alloc(f);
    }
    let output =
        crate::backend::compile_program(p, &Options::default().with_string_capacity(64)).unwrap();
    execute(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){uint8_t a[]={0,128,255},b[]={0,128,254};EmelaBytes x={a,3,NULL},y={b,3,NULL};char buffer[64];if(!equal(x,x)||equal(x,y))return 1;EmelaString s=display(x,buffer,sizeof(buffer));const char expected[]="Bytes.hex(\"0080ff\")";if(s.length!=sizeof(expected)-1||memcmp(s.data,expected,s.length)||s.owner)return 2;}return 0;}
"#,
        0,
    );
}

#[test]
fn dynamic_defect_preserves_an_offset_string_view_and_embedded_nul() {
    let mut f = simple();
    f.ret = TypeId::NEVER;
    let text = aggregate_variable(&mut f, TypeId::STRING);
    f.params.push(text);
    f.seqs[f.body.unwrap()].term = Term::Defect(text.into());
    let output = crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
    execute(
        &output,
        "#include <stdlib.h>\n#include <string.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char expected[]={'a',0,'b'};exit(k==EMELA_DEFECT_PANIC&&n==3&&!memcmp(m,expected,3)?0:2);}\nint main(void){const char data[]={'x','a',0,'b','y'};answer((EmelaString){data+1,3,NULL});return 1;}\n",
        0,
    );
}

#[test]
fn float_primitive_cases_keep_the_fixed_old_arithmetic_comparison_and_nan_results() {
    let cases = [
        (
            PrimOp::Add,
            vec![Const::Float(1.25), Const::Float(2.75)],
            TypeId::FLOAT,
            "actual == 4.0",
        ),
        (
            PrimOp::Sub,
            vec![Const::Float(4.5), Const::Float(1.25)],
            TypeId::FLOAT,
            "actual == 3.25",
        ),
        (
            PrimOp::Mul,
            vec![Const::Float(1.5), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 3.0",
        ),
        (
            PrimOp::Div,
            vec![Const::Float(7.0), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 3.5",
        ),
        (
            PrimOp::Rem,
            vec![Const::Float(7.0), Const::Float(2.0)],
            TypeId::FLOAT,
            "actual == 1.0",
        ),
        (
            PrimOp::Neg,
            vec![Const::Float(-2.5)],
            TypeId::FLOAT,
            "actual == 2.5",
        ),
        (
            PrimOp::Eq,
            vec![Const::Float(1.25), Const::Float(1.25)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Ne,
            vec![Const::Float(1.25), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Lt,
            vec![Const::Float(-1.0), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Le,
            vec![Const::Float(2.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Gt,
            vec![Const::Float(3.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Ge,
            vec![Const::Float(2.0), Const::Float(2.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
        (
            PrimOp::Eq,
            vec![Const::Float(f64::NAN), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(0)",
        ),
        (
            PrimOp::Ne,
            vec![Const::Float(f64::NAN), Const::Float(0.0)],
            TypeId::BOOL,
            "actual == INT32_C(1)",
        ),
    ];
    for (op, args, result_type, expected) in cases {
        let mut f = simple();
        f.ret = result_type;
        let result = variable(&mut f, result_type);
        let operation = binding(
            &mut f,
            Some(result),
            Value::Prim {
                op,
                ty: PrimTy::F64,
                args: args.into_iter().map(Into::into).collect(),
            },
        );
        f.seqs[f.body.unwrap()] = Seq {
            stmts: vec![operation],
            term: Term::Return(Some(result.into())),
        };
        let output =
            crate::backend::compile_program(program(vec![f]), &Options::default()).unwrap();
        let ty = if result_type == TypeId::FLOAT {
            "double"
        } else {
            "int32_t"
        };
        let harness = format!(
            "#include <math.h>\n#include <stdint.h>\n#include \"emela_embedded.h\"\nint main(void){{{ty} actual=answer();return ({expected})?0:1;}}\n"
        );
        execute(&output, &harness, 0);
    }
}
