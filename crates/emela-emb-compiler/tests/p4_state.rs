#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn state_shares_owned_values_with_escaped_closures_and_reclaims_every_run() {
    let output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p4/state.emel"),
        )],
        "main.emel",
        &Options::default()
            .with_heap_bytes(32768)
            .with_string_capacity(256),
    )
    .unwrap_or_else(|errors| panic!("State source rejected: {errors:#?}"));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
static char input[3];static int trace;
int32_t mark(int32_t value){trace=trace*10+value;return value;}
void mutate_input(void){memset(input,'z',3);}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){
  for(int i=0;i<1000;i++){
    assert(state_counter()==83&&test_heap_empty());
    memcpy(input,"a\0b",3);
    assert(state_owned((EmelaString){input,3,NULL},(EmelaString){"a\0b",3,NULL}));
    assert(!memcmp(input,"zzz",3)&&test_heap_empty());
    assert(state_failure((EmelaString){"a\0b",3,NULL})&&test_heap_empty());
    trace=0;assert(state_order()==42&&trace==132&&test_heap_empty());
    assert(state_nested()==42&&test_heap_empty());
  }
  return 0;
}
"#,
    );
}

#[test]
fn state_update_requires_a_pure_callback_and_cells_are_not_public_types() {
    for (index, source) in [
        "import Std.State.{State}\nerror Bad\nfn fail_update(n:Int)->Int fails Bad{fail Bad}\nfn main(){State.run(0,fn(){(use State[Int]).update(fail_update)})}",
        "import Std.State.{State}\nfn main(){State.run(0,fn(){(use State[Int]).update(fn(n){(use State[Int]).get()+n})})}",
        "fn main(value:Cell[Int]){value}",
        "@intrinsic\nfn run(initial:Int,body:fn()->Int)->(Int,Int)\nfn main(){run(0,fn(){42})}",
    ].into_iter().enumerate() {
        let errors = compile(source).unwrap_err();
        let expected = ["set constraint", "set constraint", "Cell", "reserved"];
        assert!(errors.iter().all(|error| error.source.as_deref() == Some("main.emel")), "{source}\n{errors:#?}");
        assert!(errors.iter().any(|error| error.message.contains(expected[index])), "{source}\n{errors:#?}");
    }
}
