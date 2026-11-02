mod support;
use emela_emb_compiler::{ExternalAdapter, ExternalResult, Options, Source, compile_sources};

#[test]
fn opaque_external_values_keep_pointer_identity_and_const_memoization_without_heap() {
    let source = r#"
@external
@external
type Token
@external
type Handle[A]
@external(c,"host_token")
fn token()->Token use {}
@external(c,"host_handle")
fn handle[A]()->Handle[A] use {}
const TOKEN=token()
@export(c,"read_token")
pub fn read()->Token{TOKEN}
@export(c,"pass_handle")
pub fn pass(value:Handle[Int])->Handle[Int]{value}
@export(c,"new_number")
pub fn number()->Handle[Int]{handle()}
@export(c,"new_text")
pub fn text()->Handle[String]{handle()}
"#;
    for policy in [ExternalResult::Borrowed, ExternalResult::Owned] {
        let output = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default()
                .with_external_adapter("host_token", ExternalAdapter::new("board_token", policy))
                .with_external_adapter("host_handle", ExternalAdapter::new("board_handle", policy)),
        )
        .unwrap_or_else(|errors| panic!("external types rejected: {errors:#?}"));
        assert!(!output.c_source.contains("emela_heap_storage"));
        let harness = r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int object,calls,handle_calls;
void *board_token(void){calls++;return __TOKEN_RESULT__;}
void *board_handle(void){handle_calls++;return &object;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){for(int i=0;i<1000;i++){
 assert(read_token()==__TOKEN_RESULT__&&calls==1);
 assert(pass_handle(&object)==&object&&pass_handle(NULL)==NULL);
 assert(new_number()==&object&&new_text()==&object);
}assert(handle_calls==2000);return 0;}
"#;
        for result in ["&object", "NULL"] {
            support::compile_and_run(&output, &harness.replace("__TOKEN_RESULT__", result));
        }
    }
}

#[test]
fn external_types_are_nominal_and_cannot_be_constructed_or_inspected() {
    for source in [
        "@external()\ntype Token",
        "@external(c,\"Token\")\ntype Token",
        "@external\ntype Token(value:Int)",
        "@external\nenum Token{Value}",
        "@external\ntype Token derive Eq",
        "@external\ntype Token\nfn make(){Token}",
        "@external\ntype Token\nfn make(){Token()}",
        "@external\ntype Token\nfn inspect(value:Token){value.x}",
        "@external\ntype Token\nfn inspect(value:Token){value.x()}",
        "@external\ntype Token\nfn update(value:Token){Token(..value)}",
        "@external\ntype Token\nfn inspect(value:Token){value match{Token->42}}",
        "@external\ntype Handle[A]\nfn wrong(value:Handle[Int])->Handle[String]{value}",
        "@external\ntype First\n@external\ntype Second\nfn wrong(value:First)->Second{value}",
    ] {
        let errors = emela_emb_compiler::compile(source).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.kind != emela_emb_compiler::DiagnosticKind::Backend),
            "{source}\n{errors:#?}"
        );
    }
}

#[test]
fn generic_externals_reject_incompatible_concrete_c_signatures() {
    let source = r#"
@external(c,"host_identity")
fn identity[A](value:A)->A use {}
@export(c,"read_int")
pub fn number()->Int{identity(42)}
@export(c,"read_int64")
pub fn wide()->Int64{identity(Int64.from_int(42))}
"#;
    let errors = emela_emb_compiler::compile(source).unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Backend
                && error.message.contains("incompatible signatures")
        ),
        "{errors:#?}"
    );
}
