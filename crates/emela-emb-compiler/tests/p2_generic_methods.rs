mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn generic_trait_methods_instantiate_after_impl_parameters_and_default_self() {
    let source = r#"
type Box[A](value:A)
type Empty
trait Keep { fn keep[A](self,value:A)->A }
impl Box[A]:Keep { fn keep(self,value) { value } }
trait DefaultKeep {
  fn keep[A](self,value:A)->A { value }
  fn again[A](self,value:A)->A { self.keep(value) }
  fn same[A:Eq](self,left:A,right:A)->Bool { left==right }
}
impl Empty:DefaultKeep {}
fn wrapped[A:Keep,B](receiver:A,value:B)->B { receiver.keep(value) }
@export(c,"generic_method_int")
pub fn integer()->Int { wrapped(Box(True),42) }
@export(c,"generic_method_string")
pub fn text(value:String)->String { Empty.again(value) }
@export(c,"generic_method_bound")
pub fn bound()->Bool { DefaultKeep.same(Empty,left:Some(42),right:Some(42)) }
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(generic_method_int()==42);assert(generic_method_bound());EmelaString value=generic_method_string((EmelaString){"a\0b",3,NULL});assert(value.length==3&&memcmp(value.data,"a\0b",3)==0);return 0;}
"#,
    );
}

#[test]
fn generic_method_error_type_arguments_flow_through_escape_and_propagation() {
    let source = r#"
type Sensor
error Missing(code:Int)
trait Readable { fn read[E](self,err:E)->Int fails E }
impl Sensor:Readable { fn read(self,err) { fail err } }
fn load[E](sensor:Sensor,err:E)->Int fails E { Readable.read(sensor,err) }
@export(c,"generic_method_error")
pub fn read()->Int { load(Sensor,Missing(code:42)) escape { Missing(code:) -> code } }
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(1024),
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
int main(void){assert(generic_method_error()==42);return 0;}
"#,
    );
}

#[test]
fn generic_method_bounds_and_unavailable_type_parameter_names_are_rejected() {
    for source in [
        "type Item\ntrait T{fn f[A:Ord](self,x:A)->A}\nimpl Item:T{fn f(self,x){x}}\nfn bad(){T.f(Item,1.0)}",
        "type Item\ntrait T{fn f[A](self,x:A)->A}\nimpl Item:T{fn f(self,x){n:A=x;n}}",
        "type Item\ntrait T{fn f[A](self,x:A)->A}\nimpl Item:T{fn f[A](self,x){x}}",
        "trait T{fn f[A,A](self,x:A)->A}",
        "trait T{fn f[a](self,x:a)->a}",
        "type Item\ntrait T{fn f[E](self,err:E)->Int fails E}\nimpl Item:T{fn f(self,err){fail err}}\nfn bad(){T.f(Item,42)}",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
