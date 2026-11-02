//! Fixed source lowering plus the old test's explicit post-lowering Hold seam.
use super::*;

#[test]
fn cancelled_nested_scope_failures_become_notes_of_the_first_failure() {
    let original = include_str!("../../../tests/compat/p6/fiber-overlap-notes.emel");
    let source = original
        .replace("pub fn work()", "@export(c,\"work\")\npub fn work()")
        .replace("pub fn show(c:", "@export(c,\"show\")\npub fn show(c:");
    let mut modules =
        crate::frontend::modules::collect(&[crate::Source::new("main.emel", &source)], "main.emel")
            .unwrap();
    crate::frontend::consts::expand(&mut modules).unwrap();
    let checked = crate::frontend::check::check(modules).unwrap();
    let mut checked = crate::frontend::specialize::specialize(checked).unwrap();
    crate::frontend::function_abi::project(&mut checked).unwrap();
    let mut program = crate::frontend::lower::lower(&checked);
    let (_, late) = program
        .functions
        .iter_mut()
        .find(|(_, function)| function.name == "late")
        .unwrap();
    assert!(late.suspends);
    let hold = binding(
        late,
        None,
        Value::Runtime {
            op: RuntimeOp::Hold,
            args: vec![],
        },
    );
    late.seqs[late.body.unwrap()].stmts.insert(0, hold);
    let mut output = crate::backend::compile_program(
        program,
        &Options::default()
            .with_heap_bytes(8192)
            .with_fiber_capacity(10)
            .with_frame_capacity(48)
            .with_frame_bytes(1024)
            .with_task_capacity(4)
            .with_event_capacity(4)
            .with_task_adapter("host_wait", "board_wait_start"),
    )
    .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static EmelaTask tasks[4];static int count;
uint32_t emela_board_event_lock(void){return 0;}
void emela_board_event_unlock(uint32_t previous){(void)previous;}
void board_wait_start(EmelaTask task,int32_t n){assert(n>=1&&n<=4);tasks[n-1]=task;count++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){count=0;void*cause=NULL;EmelaFiber f=work_start(&cause);while(emela_scheduler_run(1)){}assert(count==4);int first=i%2?1:3,second=i%2?3:1;assert(emela_task_complete_int(tasks[first-1],0));while(emela_scheduler_run(1)){}assert(!emela_fiber_done(f));assert(emela_task_complete_int(tasks[second-1],0));while(!emela_fiber_done(f))assert(emela_scheduler_run(1));assert(cause&&!emela_task_complete_int(tasks[1],0)&&!emela_task_complete_int(tasks[3],0));emela_fiber_release(f);char buffer[256],expected[256];int length=snprintf(expected,sizeof(expected),"error: Boom(n: %d)\nnote: another fiber also failed: error: Boom(n: %d)",first,second);EmelaString text=show(cause,buffer,sizeof(buffer));assert(text.length==length&&!memcmp(text.data,expected,length));emela_embedded_release(cause);assert(test_heap_empty());}return 0;}
"#,
        0,
    );
}
