#[path = "compat/p2/float_parse_cases.rs"]
mod float_parse_cases;
mod support;

use emela_emb_compiler::{ExternalAdapter, ExternalResult, Options, Source, compile_sources};

const SOURCE: &str = r#"
@external(c, "record_parse_error")
fn record_parse_error(input: String, kind: Int) -> Unit use {}
@export(c, "parse_float")
pub fn parse_float(input: String) -> Float {
  Float.parse(input) escape {
    ParseError(input: original) -> { record_parse_error(original, 1)
      0.0 }
  }
}
@export(c, "parse_int")
pub fn parse_int(input: String) -> Int {
  Int.parse(input) escape {
    ParseError(input: original) -> { record_parse_error(original, 2)
      0 }
  }
}
@export(c, "parse_bool")
pub fn parse_bool(input: String) -> Bool {
  Bool.parse(input) escape {
    ParseError(input: original) -> { record_parse_error(original, 3)
      False }
  }
}
"#;

const HARNESS: &str = r#"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
static EmelaString expected_input;
static int expected_kind, errors;
void record_parse_error(EmelaString value, int32_t kind) {
  assert(kind == expected_kind);
  assert(value.length == expected_input.length);
  assert(!memcmp(value.data, expected_input.data, value.length));
  ++errors;
}
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) {
  fprintf(stderr, "defect %d: %.*s\n", kind, length, message); abort();
}
static uint64_t bits(double value) {
  uint64_t result; memcpy(&result, &value, sizeof result); return result;
}
"#;

fn input(harness: &mut String, text: &str, kind: u8) {
    let bytes = text
        .as_bytes()
        .iter()
        .map(|byte| format!("0x{byte:02x}"))
        .collect::<Vec<_>>()
        .join(",");
    harness.push_str(&format!(
        "{{ static const unsigned char input[] = {{{}}}; expected_input=(EmelaString){{(const char *)input,{},NULL}}; expected_kind={kind}; errors=0;\n",
        if bytes.is_empty() { "0" } else { &bytes }, text.len(),
    ));
}

#[test]
fn numeric_and_bool_parse_preserve_precision_and_original_error_input() {
    let output = compile_sources(
        &[Source::new("main.emel", SOURCE)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(8192)
            .with_external_adapter(
                "record_parse_error",
                ExternalAdapter::new("record_parse_error", ExternalResult::Borrowed),
            ),
    )
    .unwrap();
    assert!(!output.c_source.contains("strtod("));
    assert!(!output.c_source.contains("malloc("));
    let mut harness = HARNESS.to_owned();
    harness.push_str("int main(void) {\n");
    for (text, expected) in float_parse_cases::cases() {
        input(&mut harness, &text, 1);
        harness.push_str("double result=parse_float(expected_input);\n");
        match expected {
            Some(value) if value.is_nan() => {
                harness.push_str("assert(!errors && isnan(result));\n")
            }
            Some(value) => harness.push_str(&format!(
                "assert(!errors && bits(result)==UINT64_C(0x{:016x}));\n",
                value.to_bits()
            )),
            None => harness.push_str("assert(errors==1 && result==0.0);\n"),
        }
        harness.push_str("}\n");
    }
    for (text, expected) in [
        ("+2147483647", Some(i32::MAX)),
        ("-2147483648", Some(i32::MIN)),
        ("+0", Some(0)),
        ("-0", Some(0)),
        ("00042", Some(42)),
        ("2147483648", None),
        ("-2147483649", None),
        (" 1", None),
        ("1 ", None),
        ("+", None),
        ("-", None),
        ("12x", None),
        ("", None),
        ("1\0x", None),
        ("1_000", None),
        ("0xff", None),
        ("１２", None),
    ] {
        input(&mut harness, text, 2);
        harness.push_str("int32_t result=parse_int(expected_input);\n");
        match expected {
            Some(value) => {
                harness.push_str(&format!("assert(!errors && result==INT64_C({value}));\n"))
            }
            None => harness.push_str("assert(errors==1 && result==0);\n"),
        }
        harness.push_str("}\n");
    }
    for (text, expected) in [
        ("True", Some(true)),
        ("False", Some(false)),
        ("true", None),
        ("false", None),
        ("TRUE", None),
        ("False ", None),
        (" True", None),
        ("1", None),
        ("0", None),
        ("", None),
        ("True\0", None),
    ] {
        input(&mut harness, text, 3);
        harness.push_str("int32_t result=parse_bool(expected_input);\n");
        match expected {
            Some(value) => harness.push_str(&format!(
                "assert(!errors && result=={});\n",
                i32::from(value)
            )),
            None => harness.push_str("assert(errors==1 && result==0);\n"),
        }
        harness.push_str("}\n");
    }
    harness.push_str("return 0; }\n");
    support::compile_and_run(&output, &harness);
}
