mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn generic_errors_flow_through_inferred_calls_recursive_components_conditions_and_traits() {
    let source = r#"
error Payload[A](value:A)
fn raise[A](value:A) { fail Payload(value:) }
fn left[A](n:Int,value:A) { if n==0 { fail Payload(value:) } else { right(n-1,value) } }
fn right[A](n:Int,value:A) { if n==0 { fail Payload(value:) } else { left(n-1,value) } }
fn raise_text(value:String) { fail Payload(value:String.append(value,"!")) }
fn recover_first(n:Int) { recover_second(n) escape { Payload(value:) -> value+1 } }
fn recover_second(n:Int) { if n==0 { fail Payload(value:41) } else { recover_first(n-1) } }
type Reader
trait Readable { fn read[A](self,value:A)->Int fails Payload[A] }
impl Reader:Readable { fn read(self,value) { fail Payload(value:) } }
@export(c,"generic_error_call")
pub fn direct()->Int { raise(41) escape { Payload(value:) -> value+1 } }
@export(c,"generic_error_scc")
pub fn recursive()->Int { left(4,42) escape { Payload(value:) -> value } }
@export(c,"generic_error_scc_recovery")
pub fn recursive_recovery()->Int { recover_first(4) }
@export(c,"generic_error_trait")
pub fn conditional()->Int { (if True { Readable.read(Reader,42) } else { 0 }) escape { Payload(value:) -> value } }
@export(c,"generic_error_bool")
pub fn complete(value:Bool)->Int { raise(value) escape { Payload(True)->1
Payload(False)->2 } }
@export(c,"generic_error_owned_string")
pub fn owned(value:String)->Bool { raise_text(value) escape { Payload(value:message) -> message=="value!" } }
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
int main(void){assert(generic_error_call()==42);assert(generic_error_scc()==42);assert(generic_error_scc_recovery()==42);assert(generic_error_trait()==42);assert(generic_error_bool(1)==1);assert(generic_error_bool(0)==2);for(int i=0;i<1000;i++){assert(generic_error_owned_string((EmelaString){"value",5,NULL}));}return 0;}
"#,
    );
}

#[test]
fn failure_only_type_variables_are_generalized_before_callers_instantiate_them() {
    let source = r#"
error Payload[A](value:A)
fn empty() { fail Payload(value:None) }
@export(c,"error_only_int")
pub fn integer()->Int { empty() escape { Payload(Some(n))->n
Payload(None)->42 } }
@export(c,"error_only_string")
pub fn text()->String { empty() escape { Payload(Some(value))->value
Payload(None)->"none" } }
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
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(error_only_int()==42);EmelaString value=error_only_string();assert(value.length==4&&memcmp(value.data,"none",4)==0);return 0;}
"#,
    );
}

#[test]
fn inferred_error_payload_types_and_joint_guarded_coverage_remain_checked() {
    for source in [
        "error Payload[A](value:A)\nfn raise[A](value:A){fail Payload(value:)}\nfn wrong()->Int{raise(\"wrong\") escape{Payload(value:)->value+1}}",
        "error Payload[A](value:A)\nfn raise[A](value:A){fail Payload(value:)}\nfn partial()->Int{raise(True) escape{Payload(True)->1}}",
        "error Payload[A](value:A)\nfn raise[A](value:A){fail Payload(value:)}\nfn guarded()->Int{raise(True) escape{Payload(value:) if value->1\nPayload(False)->2}}",
        "error Payload[A](value:A)\nfn raise[A](value:A){fail Payload(value:)}\nfn pure()->Int{raise(42)}",
        "error Oops\nfn raise(){fail Oops}\nfn wrong()->Int{raise() escape{_ -> 0}}",
        "error Oops\nfn raise(){fail Oops}\nfn wrong()->Int{raise() escape{err -> 0}}",
        "error Oops\nconst KNOWN=Oops\nfn raise(){fail Oops}\nfn wrong()->Int{raise() escape{KNOWN -> 0}}",
        "error Payload[A](value:A)\nfn first(n:Int){second(n) escape{Payload(value:)->value+1}}\nfn second(n:Int){if n==0{fail Payload(value:\"wrong\")}else{first(n-1)}}",
    ] {
        let errors = compile(source).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.source.as_deref() == Some("main.emel")),
            "{source}"
        );
    }
}
