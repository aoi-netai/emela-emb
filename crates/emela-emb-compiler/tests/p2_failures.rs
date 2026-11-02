mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};
const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdint.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) { (void)kind;(void)message;(void)length;abort(); }
"#;
#[test]
fn a_string_function_with_only_failure_paths_keeps_a_valid_c_output_abi() {
    let source = r#"
error Missing(text: String)
fn failing(n: Int) -> String fails Missing { fail Missing(Int.to_string(n)) }
fn propagate(n: Int) -> String fails Missing { failing(n) }
@export(c, "recover_text_length")
pub fn recover_text_length(n: Int) -> Int {
  text = propagate(n) escape { Missing(text) -> text }
  String.byte_size(text)
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
        &format!(
            "{BOARD}\nint main(void) {{ for (int i=0;i<1000;i++) assert(recover_text_length(-42)==3); return 0; }}"
        ),
    );
}
#[test]
fn failures_escape_whole_expressions_and_handlers_propagate_outward() {
    let source = r#"
error Bad(value: Int)
error Other(value: Int)
error Flag(value: Bool)
fn bad(value: Int) -> Int fails Bad { fail Bad(value:) }
fn unexpected() -> Int { assert False
 0 }
fn add(a: Int,b: Int) -> Int { a+b }
fn flag(value: Bool) -> Int fails Flag { fail Flag(value:) }
@export(c,"recover")
pub fn recover() -> Int { add(bad(42),unexpected()) escape { Bad(value:) -> value } }
@export(c,"nested")
pub fn nested() -> Int { (bad(1) escape { Bad(value:) -> fail Other(value: value+41) }) escape { Other(value:) -> value } }
@export(c,"flags")
pub fn flags(value: Bool) -> Int { flag(value) escape { Flag(True) -> 41
 Flag(False) -> 42 } }
const RECOVERED = bad(40) escape { Bad(value:) -> value+2 }
@export(c,"recovered_const")
pub fn recovered_const() -> Int { RECOVERED }
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nint main(void) {{ for (int i=0;i<1000;i++) {{ assert(recover()==42);assert(nested()==42);assert(flags(1)==41);assert(flags(0)==42);assert(recovered_const()==42); }} return 0; }}"
        ),
    );
}
#[test]
fn generic_failure_types_and_inferred_failure_scc_execute() {
    let source = r#"
enum Option[A] { Some(A), None }
error Bad(value: Int)
fn or_fail[A,E](value: Option[A],err: E) -> A fails E { value match { Some(value) -> value
 None -> fail err } }
fn first(n: Int) { if n==0 { fail Bad(value:42) } else { second(n-1) } }
fn second(n: Int) { if n==0 { 0 } else { first(n-1) } }
@export(c,"generic_error")
pub fn generic_error() -> Int { or_fail(None,Bad(value:42)) escape { Bad(value:) -> value } }
@export(c,"inferred_errors")
pub fn inferred_errors() -> Int { first(10) escape { Bad(value:) -> value } }
"#;
    let output = compile(source).unwrap();
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nint main(void) {{ assert(generic_error()==42);assert(inferred_errors()==42);return 0; }}"
        ),
    );
}
#[test]
fn rejects_pure_signature_failures_partial_escape_and_non_error_parameters() {
    for source in [
        "error Bad\nfn bad() -> Int { fail Bad }",
        "error Bad\nfn bad() -> Int fails Bad { fail Bad }\nfn pure() -> Int { bad() }",
        "error Flag(value:Bool)\nfn bad() -> Int fails Flag {fail Flag(True)}\nfn pure() -> Int { bad() escape { Flag(True) -> 42 } }",
        "error Flag(value:Bool)\nfn bad() -> Int fails Flag {fail Flag(True)}\nfn pure() -> Int { bad() escape { Flag(value:) if value -> 42\n Flag(False) -> 0 } }",
        "fn bad() -> Int { fail 42 }",
        "fn bad() -> Int fails Int { 42 }",
        "enum Option[A]{Some(A),None}\nfn f[A,E](v:Option[A],e:E)->A fails E {v match {Some(x)->x\nNone->fail e}}\nfn main()->Int{f(Some(1),0) escape{_->42}}",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}
