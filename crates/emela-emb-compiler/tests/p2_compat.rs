mod support;

use emela_emb_compiler::{Options, Output, Source, compile_sources};
use support::compile_and_run;

fn source_output(source: &str, options: &Options) -> Output {
    compile_sources(&[Source::new("main.emel", source)], "main.emel", options)
        .unwrap_or_else(|errors| panic!("P2 source rejected: {errors:#?}"))
}

const STRINGS: &str = include_str!("compat/p2/strings.emel");
const UNICODE: &str = include_str!("compat/p2/unicode.emel");

#[test]
fn legacy_string_views_comparisons_append_and_bounded_returns_execute() {
    let output = source_output(STRINGS, &Options::default().with_string_capacity(64));
    assert!(!output.c_source.contains("emela_heap_storage"));
    compile_and_run(&output, include_str!("compat/p2/strings-harness.c"));
}

#[test]
fn source_string_length_matches_every_unicode_18_grapheme_vector() {
    let output = source_output(UNICODE, &Options::default().with_string_capacity(64));
    let mut harness = String::from(
        r#"
#include <stdio.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
struct vector { const char *bytes; uint16_t length; int32_t clusters; };
static const struct vector vectors[] = {
"#,
    );
    let mut cases = 0;
    for line in include_str!("data/GraphemeBreakTest-18.0.0.txt").lines() {
        let tokens = line
            .split('#')
            .next()
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>();
        if tokens.is_empty() {
            continue;
        }
        let clusters = tokens.iter().filter(|token| **token == "÷").count() - 1;
        let mut bytes = Vec::new();
        for token in tokens.iter().filter(|token| !matches!(**token, "÷" | "×")) {
            let code_point = u32::from_str_radix(token, 16).expect("Unicode vector code point");
            let scalar = char::from_u32(code_point).expect("Unicode vector scalar");
            let mut encoded = [0; 4];
            bytes.extend_from_slice(scalar.encode_utf8(&mut encoded).as_bytes());
        }
        harness.push_str("  {\"");
        for byte in &bytes {
            harness.push_str(&format!("\\x{byte:02x}"));
        }
        harness.push_str(&format!(
            "\", UINT16_C({}), INT32_C({clusters})}},\n",
            bytes.len()
        ));
        cases += 1;
    }
    assert!(cases > 800, "official Unicode fixture is incomplete");
    harness.push_str(r#"
};
int main(void) {
  for (unsigned i = 0; i < sizeof vectors / sizeof *vectors; i++) {
    EmelaString value = {vectors[i].bytes, vectors[i].length, NULL};
    int32_t actual = string_length(value);
    if (actual != vectors[i].clusters || string_byte_size(value) != vectors[i].length) {
      fprintf(stderr, "Unicode case %u: expected %d clusters, got %d\n", i, vectors[i].clusters, actual);
      return 1;
    }
  }
  return string_length((EmelaString){NULL, 0, NULL}) == 0 ? 0 : 2;
}
"#);
    compile_and_run(&output, &harness);
}

#[test]
fn malformed_utf8_string_length_raises_the_legacy_defect() {
    let output = source_output(UNICODE, &Options::default().with_string_capacity(64));
    compile_and_run(
        &output,
        r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  static const char expected[] = "invalid UTF-8 string";
  exit(k == EMELA_DEFECT_PANIC && n == sizeof expected - 1 && memcmp(m, expected, n) == 0 ? 0 : 3);
}
int main(void) { const char invalid[] = {(char)0xff}; (void)string_length((EmelaString){invalid, 1, NULL}); return 2; }
"#,
    );
}

#[test]
fn legacy_numeric_formatting_rounding_and_unused_defects_execute() {
    let output = source_output(
        include_str!("compat/p2/numeric-format.emel"),
        &Options::default().with_string_capacity(64),
    );
    assert!(!output.c_source.contains("snprintf"));
    assert!(!output.c_source.contains("malloc"));
    compile_and_run(&output, include_str!("compat/p2/numeric-format-harness.c"));
}
