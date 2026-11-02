//! Old tests/builtin_audit/mod.rs:159 keeps Runtime ShowString's capacity effect.
use super::*;

#[test]
fn discarded_runtime_show_string_still_checks_its_output_capacity() {
    let mut function = simple();
    function.ret = TypeId::UNIT;
    let input = aggregate_variable(&mut function, TypeId::STRING);
    function.params.push(input);
    let show = binding(
        &mut function,
        None,
        Value::Runtime {
            op: RuntimeOp::ShowString,
            args: vec![input.into()],
        },
    );
    function.seqs[function.body.unwrap()] = Seq {
        stmts: vec![show],
        term: Term::Return(None),
    };
    let output = crate::backend::compile_program(
        program(vec![function]),
        &Options::default().with_string_capacity(3),
    )
    .unwrap();
    execute(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int expected;
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)m;(void)n;exit(expected&&k==EMELA_DEFECT_STRING_CAPACITY?7:9);}
int main(void){for(int i=0;i<1000;i++)answer((EmelaString){"a",1,NULL});
  expected=1;answer((EmelaString){"abc",3,NULL});return 8;}
"#,
        7,
    );
}
