#[path = "support/heap.rs"]
mod heap;
mod support;

use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn intrinsic_function_values_keep_source_rewrites_and_release_wrappers() {
    let source = r#"
type Key(value: Int)
impl Key: Show { fn show(self) { "K#{self.value}" } }
type Plain(value: Int)
@external(c, "read_once")
fn read_once() -> Int
fn render[A: Show](value: A) -> String {
  f: fn(A) -> String = inspect
  f(value)
}
@export(c, "intrinsic_values")
pub fn run() -> Int {
  decimal: fn(Int) -> String = Int.to_string
  inspect_string: fn(String) -> String = inspect
  debug: fn(Plain) -> Plain = dbg
  parse: fn(String) -> Int fails ParseError = Int.parse
  assert decimal(42) == "42"
  assert inspect_string("a") == "\"a\""
  assert render(Key(7)) == "K7"
  parsed = parse("41") escape { ParseError(_) -> 0 }
  assert parsed + 1 == 42
  value = parse("bad") escape { ParseError(input) -> if input == "bad" { 42 } else { 0 } }
  assert value == 42
  debug(Plain(read_once())).value
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(8192),
    )
    .unwrap_or_else(|errors| panic!("intrinsic reference source rejected: {errors:#?}"));
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
static int reads,logs;
int32_t read_once(void){reads++;return 42;}
void emela_board_debug(const char*s,uint16_t n){const char expected[]="Plain(value: 42)";assert(n==sizeof(expected)-1&&!memcmp(s,expected,n));logs++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=1;i<=1000;i++){assert(intrinsic_values()==42);assert(reads==i&&logs==i);assert(test_heap_empty());}return 0;}
"#,
    );
}

#[test]
fn never_returning_intrinsic_values_coerce_without_running_the_continuation() {
    let output = compile(
        r#"
@external(c,"should_not_run")
fn should_not_run() -> Int
@export(c,"placeholder_value")
pub fn placeholder_value() -> Int {
  f: fn() -> Int = todo
  f()
  should_not_run()
}
@export(c,"panic_value")
pub fn panic_value() -> Int {
  f: fn(String) -> Int = panic
  f("halt42")
  should_not_run()
}
"#,
    )
    .unwrap_or_else(|errors| panic!("Never function value rejected: {errors:#?}"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;static int defects;
int32_t should_not_run(void){abort();}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){
  if(defects==0){assert(k==EMELA_DEFECT_TODO);assert(n==19&&!memcmp(m,"not yet implemented",19));}
  else{assert(k==EMELA_DEFECT_PANIC);assert(n==6&&!memcmp(m,"halt42",6));}
  defects++;longjmp(jump,1);
}
int main(void){if(!setjmp(jump)){placeholder_value();abort();}if(!setjmp(jump)){panic_value();abort();}assert(defects==2);return 0;}
"#,
    );
}

#[test]
fn function_value_bounds_and_call_shapes_are_checked() {
    for source in [
        "type Plain(value: Int)\nfn main() { f: fn(Plain) -> String = inspect\nf(Plain(1)) }",
        "fn identity[A](value:A)->A{value}\nfn main(){f:fn(Int)->Int=identity\nf(\"bad\")}",
        "fn main(){f:fn(Int)->Option[Int]=Some\nf()}",
        "fn main(){f:fn(Int,Int)->Ordering=Ord.compare\nf(self:1,other:2)}",
        "fn keep[A:Ord](value:A)->A{value}\nfn main(){f=keep\nf(1.0)}",
        "type Ordered[A:Ord](value:A)\nfn main(){f=Ordered\nf(1.0)}",
        "type Ordered[A:Ord](value:A)\nfn invalid(f:fn()->Ordered[Float]){0}",
        "type Ordered[A:Ord](value:A)\nfn invalid(f:fn(Ordered[Float])->Int){0}",
    ] {
        assert!(
            compile(source).is_err(),
            "invalid function value accepted: {source}"
        );
    }
}

#[test]
fn later_calls_resolve_bounded_function_constructor_and_trait_references() {
    let source = r#"
type Ordered[A: Ord](value: A)
fn keep[A: Ord](value: A) -> A { value }
fn relay[A: Ord](value: A) -> A {
  f = keep
  f(value)
}
@export(c, "deferred_bounds")
pub fn run() -> Int {
  f = keep
  constructor = Ordered
  compare = Ord.compare
  show = inspect
  assert compare(20, 22) == Less
  assert show(42) == "42"
  relay(constructor(f(42)).value)
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(8192),
    )
    .unwrap_or_else(|errors| panic!("delayed reference bounds rejected: {errors:#?}"));
    let output = heap::with_heap_audit(output);
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,n,m);abort();}
int main(void){for(int i=0;i<1000;i++){assert(deferred_bounds()==42);assert(test_heap_empty());}return 0;}
"#,
    );
}
