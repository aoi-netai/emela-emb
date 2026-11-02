mod support;

use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn standard_parameter_names_and_float_total_order_match_fixed_core() {
    let source = r#"
@export(c, "standard_api")
pub fn standard_api() -> Int {
  assert String.length(s: "é") == 1
  assert String.byte_size(s: "é") == 3
  assert String.append(b: "b", a: "a") == "ab"
  assert String.contains(sub: "b", s: "abc")
  assert String.starts_with(prefix: "a", s: "abc")
  assert String.ends_with(suffix: "c", s: "abc")
  assert String.trim(s: " a ") == "a"
  assert Bool.to_string(b: True) == "True"
  assert Float.floor(x: 1.9) == 1
  assert Float.ceil(x: 1.1) == 2
  assert Float.round(x: -1.5) == -2
  assert Float.truncate(x: -1.9) == -1
  assert Float.to_string(x: -0.0) == "0.0"
  assert Int.min(b: 2, a: 1) == 1
  assert Int64.to_int(n: Int64.max(b: 2, a: 1)) == 2
  parsed = Int.parse(s: "42") escape { ParseError(_) -> -1 }
  assert parsed == 42
  assert Option.is_none(opt: Int.checked_add(b: 1, a: Int.MAX))
  assert Option.or(default: "bad", opt: String.from_code_point(n: 0x1f600)) == "😀"
  assert Option.is_none(opt: String.from_code_point(n: 0xd800))
  assert Option.is_some(opt: String.from_code_point(n: 0x10ffff))
  nan = Float.parse(s: "NaN") escape { ParseError(_) -> 0.0 }
  assert Float.is_nan(x: nan)
  assert Float.compare(a: nan, b: nan) == Equal
  assert Float.compare(a: nan, b: 1.0) == Greater
  assert Float.compare(a: 1.0, b: nan) == Less
  assert Float.min(a: nan, b: 1.0) == 1.0
  assert Float.is_nan(x: Float.max(a: 1.0, b: nan))
  assert Float.to_string(x: Float.min(a: -0.0, b: 0.0)) == "0.0"
  assert Float.to_string(x: Float.max(a: -0.0, b: 0.0)) == "0.0"
  assert Float.to_string(x: Float.abs(x: -0.0)) == "0.0"
  42
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
        r#"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  fprintf(stderr, "defect %d: %.*s\n", k, n, m); abort();
}
int main(void) { assert(standard_api() == 42); return 0; }
"#,
    );
}

#[test]
fn standard_private_intrinsics_and_unknown_parameter_labels_are_rejected() {
    for source in [
        "fn main() { Int.parse_or_none(\"1\") }",
        "fn main() { Float.parse_or_none(\"1\") }",
        "fn main() { String.contains(needle: \"a\", text: \"abc\") }",
        "fn main() { Float.floor(n: 1.0) }",
        "fn main() { Bool.to_string(value: True) }",
    ] {
        let diagnostics = compile(source).unwrap_err();
        assert!(
            diagnostics
                .iter()
                .any(|d| d.source.as_deref() == Some("main.emel")),
            "{diagnostics:?}"
        );
    }
}

#[test]
fn code_point_capacity_defects_are_preserved_when_only_the_tag_or_no_value_is_used() {
    let source = r#"
@export(c,"code_point_tag")
pub fn code_point_tag(n: Int) -> Int {
  String.from_code_point(n) match { Some(_) -> 0
    None -> 1 }
}
@export(c,"discarded_code_point")
pub fn discarded_code_point(n: Int) -> Int {
  String.from_code_point(n)
  42
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(4096)
            .with_string_capacity(2),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static jmp_buf jump;
static int defects;
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char*message,uint16_t length) {
  assert(kind==EMELA_DEFECT_STRING_CAPACITY);
  assert(length==24&&!memcmp(message,"string capacity exceeded",length));
  defects++;
  longjmp(jump,1);
}
int main(void) {
  assert(code_point_tag(-1)==1&&code_point_tag(0xd800)==1);
  for(int i=0;i<1000;i++)assert(code_point_tag(0x7ff)==0&&discarded_code_point(0x7ff)==42);
  if(!setjmp(jump)){(void)code_point_tag(0x20ac);abort();}
  if(!setjmp(jump)){(void)discarded_code_point(0x20ac);abort();}
  assert(defects==2);
  return 0;
}
"#,
    );
}
