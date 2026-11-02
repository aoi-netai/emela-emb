mod support;
use emela_emb_compiler::{Options, Source, compile_sources};
const TRAITS: &str = include_str!("compat/p2/traits_sync.emel");
#[test]
fn source_traits_defaults_generics_and_derived_fields_preserve_custom_methods() {
    let output = compile_sources(
        &[Source::new("main.emel", TRAITS)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){char out[256];const char *expected="(Envelope(key: K7, text: \"x\"), Some(K7), Present(K7, \"x\"))";for(int i=0;i<100;i++){EmelaString s=sync_traits(7,(EmelaString){"x",1,NULL},out,256);assert(s.length==strlen(expected));assert(memcmp(s.data,expected,s.length)==0);}EmelaString q=quoted((EmelaString){"x",1,NULL},out,256);assert(q.length==3&&memcmp(q.data,"\"x\"",3)==0);return 0;}
"#,
    );
}

#[test]
fn fallible_trait_methods_preserve_declared_errors_in_pure_inferred_and_escape_callers() {
    let base = r#"
type Sensor
error ReadFailed(code: Int)
trait Readable { fn read(self) -> Int fails ReadFailed }
impl Sensor: Readable { fn read(self) { fail ReadFailed(code:42) } }
"#;
    let source = format!(
        "{base}\nfn inferred() {{ Readable.read(Sensor) }}\n@export(c,\"trait_error\")\npub fn recovered() -> Int {{ Readable.read(Sensor) escape {{ ReadFailed(code:) -> code }} }}\n@export(c,\"trait_inferred\")\npub fn inferred_recovery() -> Int {{ inferred() escape {{ ReadFailed(code:) -> code }} }}"
    );
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default().with_heap_bytes(1024),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(trait_error()==42);assert(trait_inferred()==42);return 0;}
"#,
    );
    let invalid = format!("{base}\nfn pure() -> Int {{ Readable.read(Sensor) }}");
    let errors = emela_emb_compiler::compile(&invalid).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.kind == emela_emb_compiler::DiagnosticKind::Type)
    );
}

#[test]
fn invalid_impls_supertraits_bounds_and_derives_are_rejected_before_lowering() {
    for source in [
        "type Key(value:Int)\ntrait Score{fn score(self)->Int}\nimpl Key:Score{}\nfn main(){0}",
        "type Key(value:Int)\nimpl Key:Eq{fn eq(self,other){True}}\nimpl Key:Eq{fn eq(self,other){False}}\nfn main(){0}",
        "trait First:Second{}\ntrait Second:First{}\nfn main(){0}",
        "type Key(value:Int)\ntrait Ranked:Eq{fn rank(self)->Int}\nimpl Key:Ranked{fn rank(self){1}}\nfn main(){0}",
        "type Key(value:Int) derive Eq\nimpl Key:Eq{fn eq(self,other){True}}\nfn main(){0}",
        "type FloatKey(value:Float) derive Eq,Ord\nfn main(){0}",
        "type Ordered[A:Ord](value:A)\nfn main(){Ordered(value:1.0)}",
        "type Plain(value:Int)\nfn compare[A:Eq](value:A)->Bool{value==value}\nfn main(){compare(Plain(1))}",
        "fn compare[A:Ord](value:A)->Bool{value<value}\nfn main(){compare(1.0)}",
        "type Key(value:Int)\ntrait Readable{fn read(self)->Int}\nerror Oops\nimpl Key:Readable{fn read(self){fail Oops}}\nfn main(){0}",
        "fn wrong[Self](value:Self)->Self{value}",
    ] {
        assert!(emela_emb_compiler::compile(source).is_err(), "{source}");
    }
}

#[test]
fn trait_impl_selection_uses_later_constraints_without_defaulting_unknown_self() {
    let base = r#"
trait Parse { fn parse(text:String)->Self }
impl Int:Parse { fn parse(text) { Int.parse(text) escape { ParseError(_) -> 0 } } }
impl String:Parse { fn parse(text) { text } }
"#;
    let source = format!(
        r#"{base}
fn inferred() {{ value=Parse.parse("41")
value+1 }}
@export(c,"delayed_trait_int")
pub fn integer()->Int {{ inferred() }}
@export(c,"delayed_trait_string")
pub fn text()->String {{ value=Parse.parse("x")
String.append(value,"!") }}
"#
    );
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default().with_heap_bytes(1024),
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
int main(void){char out[8];assert(delayed_trait_int()==42);EmelaString value=delayed_trait_string(out,8);assert(value.length==2&&memcmp(value.data,"x!",2)==0);return 0;}
"#,
    );
    for body in ["Parse.parse(\"x\")", "Parse.parse(\"x\")\n()"] {
        let invalid = format!("{base}\nfn unknown(){{{body}}}");
        let errors = emela_emb_compiler::compile(&invalid).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.kind == emela_emb_compiler::DiagnosticKind::Type)
        );
    }
}
