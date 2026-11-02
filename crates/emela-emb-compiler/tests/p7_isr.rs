mod support;

use emela_emb_compiler::{ExternalAdapter, ExternalResult, Options, Source, compile_sources};

fn compile(source: &str, options: &Options) -> emela_emb_compiler::Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options).unwrap()
}

#[test]
fn isr_entries_use_emela_names_and_require_the_actual_external_adapter_symbol() {
    let source = r#"
@external(c,"host_tick")
fn raw_tick(value:Int)->Int use {}
@external(c,"host_float")
fn raw_float(value:Float)->Float use {}
fn helper(value:Int)->Int {raw_tick(value)}
@export(c,"TIM2_IRQHandler")
pub fn on_timer(value:Int)->Int {helper(value)}
@export(c,"float_bits")
pub fn on_float(value:Float)->Float {raw_float(value)}
"#;
    let options = Options::default()
        .with_heap_bytes(0)
        .with_isr_entry("on_timer")
        .with_isr_entry("on_float")
        .with_external_adapter(
            "host_tick",
            ExternalAdapter::new("board_tick", ExternalResult::Owned),
        )
        .with_external_adapter(
            "host_float",
            ExternalAdapter::new("board_float", ExternalResult::Owned),
        );
    for wrong in ["host_tick", "TIM2_IRQHandler"] {
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &options.clone().with_isr_safe_external(wrong),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("board_tick")
                    && error.message.contains("not marked ISR-safe")),
            "{errors:?}"
        );
    }
    let output = compile(
        source,
        &options
            .with_isr_safe_external("board_tick")
            .with_isr_safe_external("board_float"),
    );
    let manifest: serde_json::Value = serde_json::from_str(
        &output
            .files
            .iter()
            .find(|file| file.path == "emela_artifact.json")
            .unwrap()
            .contents,
    )
    .unwrap();
    assert_eq!(
        manifest["metadata"]["isr_entries"],
        serde_json::json!(["on_timer", "on_float"])
    );
    assert_eq!(
        manifest["metadata"]["isr_safe_symbols"],
        serde_json::json!(["board_float", "board_tick"])
    );
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <string.h>
#include "emela_embedded.h"
int32_t board_tick(int32_t value){return value;}
double board_float(double value){return value;}
int main(void){
 const uint64_t bits[]={0,UINT64_C(0x8000000000000000),UINT64_C(0x7ff0000000000000),UINT64_C(0xfff0000000000000),UINT64_C(0x7ff8000000000042),1,UINT64_C(0x7fefffffffffffff)};
 for(int i=0;i<1000;i++){
  assert(TIM2_IRQHandler(INT32_MIN)==INT32_MIN);
  assert(TIM2_IRQHandler(INT32_MAX)==INT32_MAX);
  for(unsigned j=0;j<sizeof(bits)/sizeof(*bits);j++){
   double value;memcpy(&value,&bits[j],sizeof(value));value=float_bits(value);
   uint64_t result;memcpy(&result,&value,sizeof(result));assert(result==bits[j]);
  }
 }
 return 0;
}
"#,
    );
}

#[test]
fn isr_checks_reject_reachable_heap_dynamic_calls_recursion_and_runtime_initialization() {
    let cases = [
        (
            "fn helper(value:Int)->Int {if value==0 {0} else {1+helper(value-1)}}\n@export(c,\"run\")\npub fn entry(value:Int)->Int {helper(value)}",
            "recursive call",
        ),
        (
            "@export(c,\"run\")\npub fn entry(value:Int)->List[Int] {[value]}",
            "heap-backed",
        ),
        (
            "type Packed(text:String)\nfn helper(value:Int)->Packed {Packed(\"#{value}\")}\n@export(c,\"run\")\npub fn entry(value:Int)->Packed {helper(value)}",
            "heap-backed",
        ),
        (
            "fn helper(f:fn(Int)->Int fails {} use {},value:Int)->Int {f(value)}\n@export(c,\"run\")\npub fn entry(f:fn(Int)->Int fails {} use {},value:Int)->Int {helper(f,value)}",
            "dynamic call",
        ),
        (
            "@external(c,\"board_seed\")\nfn seed()->Int use {}\nconst VALUE=seed()\n@export(c,\"run\")\npub fn entry()->Int {VALUE}",
            "runtime const initialization",
        ),
    ];
    for (source, expected) in cases {
        let options = Options::default()
            .with_heap_bytes(8192)
            .with_isr_safe_external("board_seed");
        compile(source, &options);
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &options.with_isr_entry("entry"),
        )
        .err()
        .unwrap_or_else(|| panic!("ISR unexpectedly accepted {expected}: {source}"));
        assert!(
            errors.iter().any(
                |error| error.message.contains(expected) && error.message.contains("ISR entry")
            ),
            "{expected}: {errors:?}"
        );
    }
}

#[test]
fn isr_checks_preserve_static_consts_borrowed_strings_and_unreachable_functions() {
    let source = r#"
import Std.Bytes.{Bytes}
const ANSWER=40+2
fn unreachable(value:Int)->Int {1+unreachable(value)}
@export(c,"read_static")
pub fn read_static()->Int {ANSWER}
@export(c,"compare")
pub fn compare(left:String,right:String)->Bool {left<right}
@export(c,"bytes_size")
pub fn bytes_size(value:Bytes)->Int {Bytes.size(value)}
"#;
    let output = compile(
        source,
        &Options::default()
            .with_heap_bytes(0)
            .with_isr_entry("read_static")
            .with_isr_entry("compare")
            .with_isr_entry("bytes_size"),
    );
    assert!(!output.c_source.contains("EmelaConstState"));
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include "emela_embedded.h"
int main(void){for(int i=0;i<1000;i++){assert(read_static()==42);assert(bytes_size((EmelaBytes){.length=70000})==70000);assert(compare((EmelaString){.data="a\0b",.length=3},(EmelaString){.data="a\0c",.length=3}));assert(!compare((EmelaString){.data="z",.length=1},(EmelaString){.data="a",.length=1}));}return 0;}
"#,
    );
    for name in ["missing", "unreachable"] {
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default().with_isr_entry(name),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("must name an emitted exported")),
            "{errors:?}"
        );
    }
}

#[test]
fn isr_checks_reject_non_scalar_external_adapters_for_all_ownership_policies() {
    let source = r#"
@external(c,"host_string")
fn host(value:String)->String use {}
@export(c,"entry")
pub fn entry(value:String)->String {host(value)}
"#;
    for policy in [
        ExternalResult::Borrowed,
        ExternalResult::Owned,
        ExternalResult::StringBuffer,
    ] {
        let options = Options::default()
            .with_heap_bytes(8192)
            .with_external_adapter("host_string", ExternalAdapter::new("board_string", policy));
        compile(source, &options);
        let errors = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &options
                .with_isr_entry("entry")
                .with_isr_safe_external("board_string"),
        )
        .unwrap_err();
        assert!(
            errors.iter().any(
                |error| error.message.contains("non-scalar external adapter")
                    || error.message.contains("heap-backed")
            ),
            "{errors:?}"
        );
    }
}
