#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};

fn fixture(original: &str, options: &Options) -> Output {
    let mut source = String::new();
    for original_line in original.split_inclusive('\n') {
        // Old Roots::Public also selected private main as an implicit C entry.
        // Only that root gains public declaration metadata; its body is untouched.
        let public_main;
        let line = if original_line.starts_with("fn main(") {
            public_main = format!("pub {original_line}");
            public_main.as_str()
        } else {
            original_line
        };
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header.split('(').next().unwrap();
            source.push_str(&format!("@export(c,\"legacy_{name}\")\n"));
        }
        source.push_str(line);
    }
    let mut output = compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    if output.c_source.contains("emela_heap_storage") {
        return heap::with_heap_audit(output);
    }
    let source = &mut output
        .files
        .iter_mut()
        .find(|f| f.path == "emela_embedded.c")
        .unwrap()
        .contents;
    source.push_str("\nint test_heap_empty(void){return 1;}\n");
    output.c_source = source.clone();
    output
}
const HEADERS: &str = r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
void emela_platform_start(void(*program)(void)){program();}
void emela_board_init(void){}
void emela_board_idle(void){}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"defect %d: %.*s\n",k,(int)n,m);abort();}
"#;

#[test]
fn legacy_scalar_closure_evidence_state_and_control_sources_keep_old_budgets() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/751aab9621d7200844457b2d0750b398ba29f34ec74d6e3f3c309175aee816a2.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_apply(40)==42);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/038a9c160aabd27172ce2d8ae4bd2f8f46ce1f8ace172ae549dd868766f85e1b.emel"
        ),
        &Options::default().with_heap_bytes(4096),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_apply(40)==42);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/8eab40d3d50b7b5ae57555df046c24872fbb6a5464a1513f4fa8d973c51170ec.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_main()==83);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/f38833ae543c2e6ae94205cdc0a8a539e39997397a486215859291c76000baf0.emel"
        ),
        &Options::default().with_heap_bytes(4096),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_apply(40)==42);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/907032ecd7f3419a1812be221308f0d9285d409ec55e9dc61715fa6dc320548f.emel"
        ),
        &Options::default().with_heap_bytes(4096),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_main()==82);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/d9d8c32030891a0372b787291a80609d182174a9394eb4288660ca0d5d072e0a.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_classify(i)==(i?i+1:0));assert(legacy_length()==3);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/9b845aa81a15d730223fde291ab2c0b11ecde36d3df5a6017518e5df66f4a51c.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_calculate(40)==42);assert(test_heap_empty());}}return 0;}}
"#
        ),
    );
}

#[test]
fn legacy_shared_state_owns_child_updates_results_failures_and_cancelled_captures() {
    let mut output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/596bf6715c9615a3f27037fbc12bb630406c8c90d4d5a73ab83d7491ea0659da.emel"
        ),
        &Options::default()
            .with_heap_bytes(8192)
            .with_string_capacity(128)
            .with_fiber_capacity(8)
            .with_frame_capacity(24)
            .with_frame_bytes(1024)
            .with_task_capacity(2)
            .with_event_capacity(2)
            .with_deferred_capacity(0)
            .with_task_adapter("host_wait", "board_wait_start"),
    );
    let c = &mut output
        .files
        .iter_mut()
        .find(|f| f.path == "emela_embedded.c")
        .unwrap()
        .contents;
    c.push_str("\nvoid test_cancel(EmelaFiber f){emela_fiber_get(f)->cancelled=1;}\n");
    output.c_source = c.clone();
    support::compile_and_run(
        &output,
        &format!(
            "{HEADERS}\n{}",
            include_str!("compat/p7/closures/shared-state.c")
        ),
    );
}

#[test]
fn legacy_captured_string_producers_keep_owners_and_eleven_byte_scratch() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/08a1e8380fcaae263306f70073caadbfce6a994072c22581d5abb902a63c7d4c.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    assert!(output.c_source.contains("[11]"));
    assert!(!output.c_source.contains("char emela_string_buffer_0[256]"));
    let header = &output
        .files
        .iter()
        .find(|f| f.path == "emela_embedded.h")
        .unwrap()
        .contents;
    let ty = header
        .lines()
        .find(|line| line.contains("legacy_formatter("))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .strip_prefix("emela_function_type_")
        .unwrap();
    let harness = include_str!("compat/p7/closures/captured-strings.c").replace("{ty}", ty);
    support::compile_and_run(&output, &format!("{HEADERS}\n{harness}"));
}

#[test]
fn legacy_string_plans_preserve_alias_and_consumer_observations() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/8b5c5d8f9f7c21c0650488a87616d0dff77342a5059bba6fe962eac6d1ce800c.emel"
        ),
        &Options::default().with_string_capacity(64),
    );
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        &format!(
            "{HEADERS}\n{}",
            include_str!("compat/p7/closures/string-plans.c")
        ),
    );
}

#[test]
fn legacy_string_helpers_preserve_alias_and_consumer_observations() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/94acedcb0cb510115d20ea32a8614be8ed88f29a241166aca4d0371835f30099.emel"
        ),
        &Options::default(),
    );
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        &format!(
            "{HEADERS}\n{}",
            include_str!("compat/p7/closures/string-helpers.c")
        ),
    );
}
