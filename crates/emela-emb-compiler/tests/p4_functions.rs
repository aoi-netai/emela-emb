mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn source_function_references_pattern_lambdas_captures_and_indirect_calls_execute() {
    let source = r#"
fn identity[A](value:A)->A {value}
fn apply(f:fn(Int)->Int,value:Int)->Int {f(value)}
fn construct(f:fn(Int)->Option[Int])->Int {f(42) match {Some(n)->n
None->0}}
fn pair(f:fn((Int,Int))->Int)->Int {f((20,22))}
fn units(f:fn((),Int,())->Int)->Int {f((),42,())}
fn maker(offset:Int)->fn(Int)->Int {fn(n){offset+n}}
fn text(value:String)->fn()->String {owned=String.append(value,"!")
fn(){owned}}
type Callback(run:fn(Int)->Int)
type Pair(left:Int,right:Int)
fn compare[A:Ord](left:A,right:A)->Ordering {cmp=Ord.compare
cmp(left,right)}
@export(c,"function_values")
pub fn run()->Int {
  f=maker(2)
  assert apply(identity,42)==42
  assert apply(fn(n){n+1},41)==42
  assert construct(Some)==42
  assert pair(fn((a,b)){a+b})==42
  pair_sum=fn(Pair(left,right)){left+right}
  assert pair_sum(Pair(20,22))==42
  assert units(fn(_,n,_){n})==42
  assert compare(1,2)==Less
  assert text("hello")()=="hello!"
  callback=Callback(run:identity)
  assert callback.run(42)==42
  stringify=Int.to_string
  assert stringify(42)=="42"
  f(40)
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
int main(void){for(int i=0;i<1000;i++){assert(function_values()==42);}return 0;}
"#,
    );
}

#[test]
fn function_values_keep_arity_positional_calls_and_irrefutable_parameters_checked() {
    for source in [
        "fn f()->Int{g=fn(n:Int){n}\ng()}",
        "fn f()->Int{g=fn(n:Int){n}\ng(n:42)}",
        "fn f(){g=fn(Some(n):Option[Int]){n}\n()}",
        "fn f(){g=fn((n,n):(Int,Int)){n}\n()}",
        "fn f()->Int{value=42\nvalue()}",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
