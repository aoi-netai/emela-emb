mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn hidden_and_explicit_failure_rows_propagate_and_pure_functions_widen() {
    let source = r#"
error Oops
fn invoke(f:fn()->Int)->Int {f()}
fn explicit[E](f:fn()->Int fails E)->Int fails E {f()}
fn fail_now()->Int fails Oops {fail Oops}
fn pure()->Int {42}
fn wider(f:fn()->Int fails Oops)->Int fails Oops {f()}
@export(c,"failure_rows")
pub fn run()->Int {
  a=invoke(fail_now) escape {Oops->20}
  b=explicit(fail_now) escape {Oops->22}
  assert invoke(pure)==42
  assert (wider(pure) escape {Oops->0})==42
  a+b
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){assert(failure_rows()==42);}return 0;}
"#,
    );
}

#[test]
fn closed_failure_rows_and_value_kinds_remain_checked() {
    for source in [
        "error Oops\nfn fail_now()->Int fails Oops{fail Oops}\nfn pure(f:fn()->Int fails {})->Int{f()}\nfn run()->Int{pure(fail_now)}",
        "fn bad[A:Immediate](value:A)->A{value}",
        "fn bad[E:Immediate](f:fn()->Int fails E)->Int fails E{f()}",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
