//! Finite recursive source shapes and the old generic Show cardinality assertions.
use super::*;

fn source_program(original: &str, name: &str) -> Program {
    let source = original.replace(
        &format!("pub fn {name}("),
        &format!("@export(c,\"legacy_{name}\")\npub fn {name}("),
    );
    let mut modules =
        crate::frontend::modules::collect(&[crate::Source::new("main.emel", &source)], "main.emel")
            .unwrap();
    crate::frontend::consts::expand(&mut modules).unwrap();
    let checked = crate::frontend::check::check(modules).unwrap();
    let mut checked = crate::frontend::specialize::specialize(checked).unwrap();
    crate::frontend::function_abi::project(&mut checked).unwrap();
    crate::frontend::lower::lower(&checked)
}

const HEADERS: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
int test_heap_empty(void);
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
"#;

fn audit_heap(output: &mut Output) {
    let source = &mut output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents;
    source.push_str("\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    output.c_source = source.clone();
}

#[test]
fn finite_recursive_source_shapes_preserve_old_layout_and_heap_budgets() {
    let program = source_program(
        include_str!(
            "../../../tests/compat/legacy-corpus/sources/31ff51508cbd03e7ee8068b0ea50b1e9d4fc758df770d7dc88d43b8e6489ff81.emel"
        ),
        "work",
    );
    let mut output =
        crate::backend::compile_program(program, &Options::default().with_heap_bytes(4096))
            .unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{assert(legacy_work(0)==42&&test_heap_empty());assert(legacy_work(1)==3&&test_heap_empty());}}return 0;}}
"#
        ),
        0,
    );

    let program = source_program(
        include_str!(
            "../../../tests/compat/legacy-corpus/sources/a3a971f303c79f46b108f2208b4655d6b8661355fcce7147f67f657be400fd07.emel"
        ),
        "make",
    );
    let output =
        crate::backend::compile_program(program, &Options::default().with_heap_bytes(2048))
            .unwrap();
    // The old value_audit case checks successful C generation only. Verify its
    // emitted public aggregate layout in strict C, without claiming a value run.
    execute(
        &output,
        &format!("{HEADERS}\nint main(void){{return 0;}}\n"),
        0,
    );
}

#[test]
fn nested_generic_error_source_keeps_one_finite_show_mapping_and_exact_text() {
    let program = source_program(
        include_str!(
            "../../../tests/compat/legacy-corpus/sources/1daea07d6938ba513abc8cc3291e8726a3ed116585528191564b19456d89ad21.emel"
        ),
        "show",
    );
    let options = Options::default()
        .with_heap_bytes(2048)
        .with_string_capacity(128);
    // The source Program also holds unrelated compiler-owned declarations. The
    // old Roots::Public bound applies to the concrete reachable function set.
    let lowered = super::super::lower_program(program.clone(), &options).unwrap();
    assert!(lowered.emitted_functions.len() < 64);
    let shows = lowered
        .lowered
        .error_shows
        .iter()
        .filter(|(_, function)| lowered.emitted_functions.contains(function))
        .collect::<Vec<_>>();
    assert_eq!(shows.len(), 1);
    assert_eq!(lowered.lowered.core.enums[shows[0].0].name, "Main.Boom");
    let mut output = crate::backend::compile_program(program, &options).unwrap();
    audit_heap(&mut output);
    execute(
        &output,
        &format!(
            r#"{HEADERS}
int main(void){{for(int i=0;i<1000;i++){{char buffer[128];EmelaString text=legacy_show(buffer,sizeof buffer);static const char expected[]="error: Boom(value: Box([42]))";assert(text.length==sizeof expected-1&&!memcmp(text.data,expected,sizeof expected-1));assert(test_heap_empty());}}return 0;}}
"#
        ),
        0,
    );
}
