use super::*;

fn clock_binding(params: &[TypeId], ret: TypeId, symbol: &str) -> Program {
    let mut p = program(vec![]);
    let mut external = Function::new("Embedded.Board.clock_now");
    external.ret = ret;
    for &ty in params {
        let param = variable(&mut external, ty);
        external.params.push(param);
    }
    external.external = Some(External {
        bindings: vec![ExternalBinding {
            target: "c".into(),
            host: HostRef::Global(symbol.into()),
        }],
    });
    let target = p.functions.alloc(external);
    let mut f = simple();
    f.ret = ret;
    let args = params
        .iter()
        .map(|&ty| {
            let param = variable(&mut f, ty);
            f.params.push(param);
            param.into()
        })
        .collect();
    let result = variable(&mut f, ret);
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
    f.seqs[f.body.unwrap()] = Seq {
        stmts: vec![call],
        term: Term::Return(Some(result.into())),
    };
    p.functions.alloc(f);
    p
}

#[test]
fn board_clock_now_without_a_handler_requires_no_task_runtime() {
    let output = crate::backend::compile_program(
        clock_binding(&[], TypeId::INT64, "__emela_peripheral_clock_now"),
        &Options::default(),
    )
    .unwrap();
    assert!(!output.header.contains("EMELA_HAS_TASK_RUNTIME"));
    assert!(!output.header.contains("EmelaTask"));
    assert!(output.files.iter().any(|file| file.path == "emela_clock.h"));
    execute(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_clock.h"
#include "emela_board.h"
#ifdef EMELA_HAS_TASK_RUNTIME
#error "Clock.now must not require the task runtime"
#endif
static unsigned reads;
int64_t emela_board_clock_now(void){reads++;return INT64_C(1700000000123);}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(unsigned i=0;i<1000;i++)assert(answer()==INT64_C(1700000000123));assert(reads==1000);return 0;}
"#,
        0,
    );
}

#[test]
fn malformed_board_bindings_and_zero_transfer_capacity_are_rejected() {
    for (params, ret, symbol, expected) in [
        (
            &[][..],
            TypeId::INT64,
            "__emela_peripheral_console_print_line",
            "invalid standard board binding identity",
        ),
        (
            &[][..],
            TypeId::INT,
            "__emela_peripheral_clock_now",
            "invalid standard board binding signature",
        ),
        (
            &[TypeId::INT][..],
            TypeId::INT64,
            "__emela_peripheral_clock_now",
            "invalid standard board binding signature",
        ),
    ] {
        let errors = crate::backend::compile_program(
            clock_binding(params, ret, symbol),
            &Options::default(),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.kind == ErrorKind::Unsupported
                    && error.message.contains(expected)),
            "{errors:#?}"
        );
        assert!(
            errors
                .iter()
                .all(|error| error.kind != ErrorKind::InvalidFlowIr),
            "{errors:#?}"
        );
    }
    assert_eq!(Options::default().peripheral_buffer_capacity(), 256);
    for capacity in [1, 256, u16::MAX] {
        crate::backend::compile_program(
            program(vec![]),
            &Options::default().with_peripheral_buffer_capacity(capacity),
        )
        .unwrap();
    }
    let errors = crate::backend::compile_program(
        program(vec![]),
        &Options::default().with_peripheral_buffer_capacity(0),
    )
    .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].kind, ErrorKind::InvalidOptions);
    assert!(
        errors[0].message.contains("peripheral_buffer_capacity")
            && errors[0].message.contains("positive")
    );
}
