mod support;
use emela_emb_compiler::{DiagnosticKind, Options, Source, compile_sources};

#[test]
fn source_without_c_roots_does_not_emit_internal_code_or_require_its_resources() {
    let output = compile_sources(
        &[Source::new(
            "library.emel",
            r#"
error Missing(input: String)
@external(c, "unused_external")
fn external_value() -> Int
pub fn visible(n: Int) -> Int { external_value() + n }
pub fn throwing(text: String) -> Int fails Missing { fail Missing(text) }
"#,
        )],
        "library.emel",
        &Options::default().with_heap_bytes(0),
    )
    .unwrap();
    assert!(output.exports.is_empty());
    assert!(!output.c_source.contains("emela_fn_"));
    assert!(!output.c_source.contains("unused_external"));
    assert!(!output.c_source.contains("emela_heap_storage"));
    support::compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
int main(void) { return 0; }
"#,
    );
    let errors = compile_sources(
        &[Source::new(
            "library.emel",
            "pub fn invalid() -> Int { True }",
        )],
        "library.emel",
        &Options::default(),
    )
    .unwrap_err();
    assert_eq!(errors[0].kind, DiagnosticKind::Type);
}
