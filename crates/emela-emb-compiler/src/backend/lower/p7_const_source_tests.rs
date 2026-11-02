//! Original source corpus through the independent frontend; optional legacy GC seam.
use super::*;
use crate::{ExternalAdapter, ExternalResult};

fn original(id: &str, exports: &[&str]) -> String {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/compat/legacy-corpus/manifest.json"
    ))
    .unwrap();
    let program = manifest["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == format!("program:{id}"))
        .unwrap();
    assert_eq!(program["files"].as_array().unwrap().len(), 1);
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/compat/legacy-corpus")
        .join(program["files"][0]["fixture"].as_str().unwrap());
    let mut source = std::fs::read_to_string(path).unwrap();
    for name in exports {
        let declaration = format!("pub fn {name}(");
        assert_eq!(source.matches(&declaration).count(), 1);
        source = source.replace(
            &declaration,
            &format!("@export(c,\"{name}\")\n{declaration}"),
        );
    }
    source
}

fn fixture(id: &str, exports: &[&str], options: &Options, gc: bool) -> Output {
    let source = original(id, exports);
    if !gc {
        return crate::compile_sources(
            &[crate::Source::new("main.emel", &source)],
            "main.emel",
            options,
        )
        .unwrap_or_else(|errors| panic!("{id}: {errors:#?}"));
    }
    let mut modules =
        crate::frontend::modules::collect(&[crate::Source::new("main.emel", &source)], "main.emel")
            .unwrap();
    crate::frontend::consts::expand(&mut modules).unwrap();
    let checked = crate::frontend::check::check(modules).unwrap();
    let mut checked = crate::frontend::specialize::specialize(checked).unwrap();
    crate::frontend::function_abi::project(&mut checked).unwrap();
    let mut program = crate::frontend::lower::lower(&checked);
    add_gc_anchor(&mut program);
    crate::backend::compile_program(program, options).unwrap()
}

pub(super) fn add_gc_anchor(program: &mut Program) {
    // Same as the fixed old gc_anchor: enable tracing through reachable Cell[Any].
    let ty = program.core.types.intern(Type::Cell(TypeId::ANY));
    let mut anchor = simple();
    anchor.name = "test_gc_anchor".into();
    anchor.export_symbol = Some(anchor.name.clone());
    anchor.ret = TypeId::ANY;
    let value = aggregate_variable(&mut anchor, TypeId::ANY);
    anchor.params.push(value);
    let cell = aggregate_variable(&mut anchor, ty);
    let result = aggregate_variable(&mut anchor, TypeId::ANY);
    let make = binding(
        &mut anchor,
        Some(cell),
        Value::Builtin {
            op: Builtin::CellNew,
            args: vec![value.into()],
        },
    );
    let read = binding(
        &mut anchor,
        Some(result),
        Value::Builtin {
            op: Builtin::CellGet,
            args: vec![cell.into()],
        },
    );
    anchor.seqs[anchor.body.unwrap()] = Seq {
        stmts: vec![make, read],
        term: Term::Return(Some(result.into())),
    };
    program.functions.alloc(anchor);
}

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){fprintf(stderr,"%u: %.*s\n",k,n,m);abort();}
"#;

fn heap_probe(output: &mut Output, gc: bool, copy_string: bool) {
    let c = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    c.push_str("\nsize_t test_const_heap(void){");
    if gc {
        c.push_str("emela_heap_collect_cycles();");
    }
    c.push_str("size_t used=0;if(!emela_heap_initialized)return 0;for(unsigned char*p=emela_heap_storage;p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)used+=b->size;p+=b->size;}return used;}\n");
    if copy_string {
        c.push_str(
            "EmelaString test_const_owned(EmelaString v){return emela_heap_copy_string(v);}\n",
        );
    }
}

#[test]
fn original_const_roots_survive_cycle_collection_and_callable_family_reads() {
    let mut output = fixture(
        "661bd10d1eb4fcbe825b",
        &["read"],
        &Options::default().with_heap_bytes(4096),
        true,
    );
    heap_probe(&mut output, true, false);
    execute(
        &output,
        &format!(
            r#"{BOARD}
static int calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
size_t test_const_heap(void);
int main(void){{assert(read()==42);size_t baseline=test_const_heap();assert(baseline);for(int i=0;i<1000;i++)assert(read()==42&&test_const_heap()==baseline);assert(calls==1);return 0;}}
"#
        ),
        0,
    );
    let names = [
        "integer",
        "text",
        "evidence",
        "failing",
        "choice_integer",
        "choice_text",
        "enum_integer",
        "enum_text",
        "caught",
    ];
    for gc in [false, true] {
        let mut output = fixture(
            "3341457f78b9863e37dd",
            &names,
            &Options::default()
                .with_heap_bytes(32768)
                .with_frame_bytes(1024),
            gc,
        );
        heap_probe(&mut output, gc, false);
        execute(
            &output,
            &format!(
                r#"{BOARD}
static unsigned calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
size_t test_const_heap(void);
int main(void){{size_t baseline=0;for(unsigned i=0;i<1001;i++){{assert(integer()==42&&text()==42&&evidence()==42&&failing(0)==42&&choice_integer()==42&&choice_text()==42&&enum_integer()==42&&enum_text()==42&&caught()==42);if(!i)baseline=test_const_heap();else assert(test_const_heap()==baseline);}}assert(calls==9);return 0;}}
"#
            ),
            0,
        );
    }
}

#[test]
fn original_external_string_const_keeps_one_snapshot_for_all_policies_and_gc_modes() {
    for policy in [
        ExternalResult::Borrowed,
        ExternalResult::Owned,
        ExternalResult::StringBuffer,
    ] {
        for gc in [false, true] {
            let mut output = fixture(
                "d81aa6294c52e64c91a5",
                &["text", "size", "discard"],
                &Options::default()
                    .with_heap_bytes(2048)
                    .with_external_adapter(
                        "host_value",
                        ExternalAdapter::new("board_value", policy),
                    ),
                gc,
            );
            heap_probe(&mut output, gc, true);
            let board = match policy {
                ExternalResult::Borrowed => {
                    "EmelaString board_value(void){calls++;return(EmelaString){.data=bytes,.length=3};}"
                }
                ExternalResult::Owned => {
                    "EmelaString board_value(void){calls++;return test_const_owned((EmelaString){.data=bytes,.length=3});}"
                }
                ExternalResult::StringBuffer => {
                    "EmelaString board_value(char*out,uint16_t capacity){calls++;assert(capacity>=3);memcpy(out,bytes,3);return(EmelaString){.data=out,.length=3};}"
                }
            };
            execute(
                &output,
                &format!(
                    r#"{BOARD}
static char bytes[]={{'4',0,'2'}};static int calls;
size_t test_const_heap(void);EmelaString test_const_owned(EmelaString);
{board}
int main(void){{discard();memset(bytes,'x',3);size_t baseline=test_const_heap();assert(baseline&&calls==1);for(int i=0;i<1000;i++){{EmelaString v=text();assert(v.length==3&&v.owner&&!memcmp(v.data,"4\0" "2",3));emela_embedded_release_string(&v);assert(size()==3&&test_const_heap()==baseline);}}assert(calls==1);return 0;}}
"#
                ),
                0,
            );
        }
    }
}

#[test]
fn original_external_aggregate_const_owns_fields_after_the_board_releases_its_value() {
    for gc in [false, true] {
        let mut output = fixture(
            "a7326c76e24e2c628792",
            &["read", "make", "first"],
            &Options::default()
                .with_heap_bytes(4096)
                .with_external_adapter(
                    "host_packet",
                    ExternalAdapter::new("board_packet", ExternalResult::Borrowed),
                ),
            gc,
        );
        let cty = output
            .header
            .lines()
            .find(|line| line.contains(" board_packet("))
            .unwrap()
            .split(" board_packet(")
            .next()
            .unwrap()
            .to_owned();
        let release = output
            .header
            .lines()
            .find(|line| {
                line.starts_with("void emela_external_release_")
                    && line.contains(&format!("({cty} *value)"))
            })
            .unwrap()
            .strip_prefix("void ")
            .unwrap()
            .split('(')
            .next()
            .unwrap()
            .to_owned();
        heap_probe(&mut output, gc, false);
        execute(
            &output,
            &format!(
                r#"{BOARD}
static int calls;static char bytes[]={{'a',0,'b'}};static {cty} held;
size_t test_const_heap(void);
{cty} board_packet(void){{calls++;held=make((EmelaString){{.data=bytes,.length=3}});return held;}}
int main(void){{{cty} value=read();{release}(&value);{release}(&held);memset(bytes,'x',3);size_t baseline=test_const_heap();for(int i=0;i<1000;i++){{value=read();assert(value.f0.length==3&&!memcmp(value.f0.data,"a\0b",3));{release}(&value);EmelaString text=first();assert(text.length==3&&!memcmp(text.data,"a\0b",3));emela_embedded_release_string(&text);assert(test_const_heap()==baseline);}}assert(calls==1);return 0;}}
"#
            ),
            0,
        );
    }
}

#[test]
fn original_const_sources_keep_recursive_closures_aggregates_and_discarded_first_reads() {
    for (id, expected, heap) in [
        ("12c836689dcf8fbdefcc", 42, 4096),
        ("fb872bf260d3593b4367", 44, 8192),
    ] {
        let output = fixture(
            id,
            &["read"],
            &Options::default().with_heap_bytes(heap),
            false,
        );
        execute(
            &output,
            &format!(
                r#"{BOARD}
static int calls;void emela_board_debug(const char*s,uint16_t n){{(void)s;(void)n;calls++;}}
int main(void){{for(int i=0;i<1000;i++)assert(read()=={expected});assert(calls==1);return 0;}}
"#
            ),
            0,
        );
    }
    let output = fixture(
        "356d9928e442971ded00",
        &["read", "discard"],
        &Options::default().with_heap_bytes(8192),
        false,
    );
    execute(
        &output,
        &format!(
            r#"{BOARD}
static int calls;int32_t fixture_seed(void){{return ++calls+41;}}
int main(void){{discard();for(int i=0;i<1000;i++)assert(read()==42);assert(calls==1);return 0;}}
"#
        ),
        0,
    );
}

#[test]
fn original_nested_const_defects_cache_the_exact_context_and_self_reference_retries() {
    let output = fixture(
        "fee491a366b1c1ad5051",
        &["bad", "self_ref"],
        &Options::default().with_heap_bytes(8192),
        false,
    );
    execute(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf target;static int calls,debug_calls,mode;
int32_t fixture_zero(void){calls++;return 0;}
void emela_board_debug(const char*s,uint16_t n){(void)s;(void)n;debug_calls++;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){const char*expected=mode?"const SELF refers to itself while being evaluated":"while evaluating const A: while evaluating const B: division by zero";assert(n==strlen(expected)&&!memcmp(m,expected,n));if(!mode)assert(k==EMELA_DEFECT_DIVISION_BY_ZERO);longjmp(target,1);}
int main(void){for(volatile int i=0;i<1000;i++)if(setjmp(target)==0){bad();abort();}assert(calls==1&&debug_calls==1);mode=1;for(volatile int i=0;i<1000;i++)if(setjmp(target)==0){self_ref();abort();}assert(calls==1001);return 0;}
"#,
        0,
    );
}

#[test]
fn original_const_heap_and_adapter_defects_are_cached_with_the_source_name() {
    let cases = [
        (
            "c52f04cf604c46c53d58",
            "read",
            Options::default().with_heap_bytes(64),
            "",
            "EMELA_DEFECT_HEAP_EXHAUSTED",
            "while evaluating const VALUE: fixed heap exhausted",
            "",
        ),
        (
            "bd83fac6076474dfdacf",
            "size",
            Options::default()
                .with_heap_bytes(1024)
                .with_string_capacity(16)
                .with_external_adapter(
                    "host_value",
                    ExternalAdapter::new("board_value", ExternalResult::StringBuffer),
                ),
            "static int calls;EmelaString board_value(char*out,uint16_t n){calls++;return (EmelaString){.data=out,.length=(uint16_t)(n+1)};}",
            "EMELA_DEFECT_STRING_CAPACITY",
            "while evaluating const VALUE: invalid external String buffer",
            "assert(calls==1);",
        ),
    ];
    for (id, root, options, producer, kind, message, after) in cases {
        let output = fixture(id, &[root], &options, false);
        execute(
            &output,
            &format!(
                r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf target;
{producer}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){{assert(k=={kind}&&n==strlen("{message}")&&!memcmp(m,"{message}",n));longjmp(target,1);}}
int main(void){{for(volatile int i=0;i<1000;i++)if(setjmp(target)==0){{{root}();abort();}}{after}return 0;}}
"#
            ),
            0,
        );
    }
}

#[test]
fn original_exponential_static_string_uses_bounded_runtime_evaluation() {
    let output = fixture(
        "2f21eb2017b3f0b1d760",
        &["size"],
        &Options::default()
            .with_heap_bytes(1024)
            .with_string_capacity(32),
        false,
    );
    execute(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){static const char prefix[]="while evaluating const VALUE: ";exit(k==EMELA_DEFECT_STRING_CAPACITY&&n>=sizeof prefix-1&&!memcmp(m,prefix,sizeof prefix-1)?0:2);}
int main(void){size();return 1;}
"#,
        0,
    );
}

#[test]
fn original_runtime_const_isr_rejection_keeps_the_static_alternative_heapless() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_isr_entry("read")
        .with_isr_safe_external("fixture_seed");
    let dynamic = original("5ce8d2ba40fb42a01466", &["read"]);
    let errors = crate::compile_sources(
        &[crate::Source::new("main.emel", &dynamic)],
        "main.emel",
        &options,
    )
    .unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("runtime const initialization")),
        "{errors:#?}"
    );
    let output = fixture(
        "7eaad73bc9a9b5a9bcaa",
        &["read"],
        &Options::default().with_isr_entry("read"),
        false,
    );
    assert!(
        !output.c_source.contains("EmelaConstState")
            && !output.c_source.contains("emela_heap_storage")
    );
    execute(
        &output,
        &format!(
            "{BOARD}\nint main(void){{for(int i=0;i<1000;i++)assert(read()==42);return 0;}}\n"
        ),
        0,
    );
}
