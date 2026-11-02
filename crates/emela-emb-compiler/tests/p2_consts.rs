mod support;
use emela_emb_compiler::compile;
#[test]
fn static_and_runtime_consts_keep_source_identity_and_initialize_once() {
    let source = r#"
@external(c, "seed")
fn seed() -> Int use {}
fn identity[A](value: A) -> A { value }
const FIRST = seed()
const SECOND = FIRST + seed()
const UNUSED = seed()
const STATIC = identity(40) + 2
@export(c, "read_const")
pub fn read_const() -> Int { SECOND + FIRST + STATIC }
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int calls;
int32_t seed(void) { return ++calls; }
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) { (void)kind;(void)message;(void)length;abort(); }
int main(void) { for (int i=0;i<1000;i++) assert(read_const()==46); assert(calls==2); return 0; }
"#,
    );
}
#[test]
fn const_defects_and_self_references_are_cached_without_tail_recur() {
    let source = r#"
@external(c, "zero")
fn zero() -> Int use {}
const BROKEN = 1 / zero()
const SELF: Int = SELF
@export(c, "broken")
pub fn broken() -> Int { BROKEN }
@export(c, "self_reference")
pub fn self_reference() -> Int { SELF }
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <setjmp.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;
static int zero_calls,defects;
static char last[1024];
int32_t zero(void) { zero_calls++; return 0; }
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) { (void)kind; assert(length<sizeof(last)); memcpy(last,message,length);last[length]=0; defects++;longjmp(jump,1); }
int main(void) { for (volatile int i=0;i<3;i++) { if (setjmp(jump)==0) { broken();abort(); } assert(strstr(last,"while evaluating const BROKEN") && strstr(last,"division by zero")); } assert(zero_calls==1 && defects==3); for (volatile int i=0;i<2;i++) { if (setjmp(jump)==0) { self_reference();abort(); } assert(strstr(last,"SELF")); } assert(defects==5);return 0; }
"#,
    );
}
#[test]
fn const_values_have_value_syntax_and_positioned_diagnostics() {
    for source in [
        "const VALUE = 42\nfn main() -> Int { VALUE() }",
        "const VALUE = 42\nconst VALUE = 43\nfn main() -> Int { VALUE }",
        "const VALUE: Int = True\nfn main() -> Int { 0 }",
    ] {
        let errors = compile(source).unwrap_err();
        assert_eq!(errors[0].source.as_deref(), Some("main.emel"));
    }
}
