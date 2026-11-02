mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

const SOURCE: &str = r#"
@export(c, "cluster_count")
pub fn cluster_count(text: String) -> Int { List.length(String.chars(text)) }
@export(c, "cluster")
pub fn cluster(text: String, index: Int) -> String {
  List.get(String.chars(text), index) match {
    Some(value) -> value
    None -> panic("missing cluster")
  }
}
@export(c, "joined")
pub fn joined(text: String) -> String { List.join(String.chars(text), "") }
@export(c, "point")
pub fn point(text: String, index: Int) -> Int { Option.or(List.get(String.code_points(text), index), -1) }
"#;

fn output(source: &str) -> emela_emb_compiler::Output {
    compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(4096)
            .with_string_capacity(64),
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"))
}

#[test]
fn every_unicode_18_cluster_boundary_and_code_point_survives_owned_lists() {
    let output = output(SOURCE);
    let mut harness = String::from(
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) { (void)k; (void)m; (void)n; abort(); }
struct vector { const char *bytes; uint16_t length, count, point_count; const uint16_t *boundaries; const int32_t *points; };
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
        let mut bytes = Vec::new();
        let mut boundaries = Vec::new();
        let mut points = Vec::new();
        for token in tokens {
            match token {
                "÷" => boundaries.push(bytes.len()),
                "×" => {}
                scalar => {
                    let point = u32::from_str_radix(scalar, 16).unwrap();
                    let scalar = char::from_u32(point).unwrap();
                    points.push(point);
                    let mut encoded = [0; 4];
                    bytes.extend_from_slice(scalar.encode_utf8(&mut encoded).as_bytes());
                }
            }
        }
        assert_eq!(boundaries.first(), Some(&0));
        assert_eq!(boundaries.last(), Some(&bytes.len()));
        assert!(bytes.len() <= 64);
        harness.push_str("{\"");
        for byte in &bytes {
            harness.push_str(&format!("\\x{byte:02x}"));
        }
        harness.push_str(&format!(
            "\", {}, {}, {}, (const uint16_t[]){{",
            bytes.len(),
            boundaries.len() - 1,
            points.len()
        ));
        for boundary in boundaries {
            harness.push_str(&format!("{boundary},"));
        }
        harness.push_str("}, (const int32_t[]){");
        for point in points {
            harness.push_str(&format!("INT32_C({point}),"));
        }
        harness.push_str("}},\n");
        cases += 1;
    }
    assert_eq!(cases, 853);
    harness.push_str(r#"
};
int main(void) {
  for (unsigned round = 0; round < 3; ++round) {
    for (unsigned i = 0; i < sizeof vectors / sizeof *vectors; ++i) {
      const struct vector *v = &vectors[i];
      EmelaString text = {v->bytes, v->length, NULL};
      assert(cluster_count(text) == v->count);
      for (int32_t index = 0; index < v->count; ++index) {
        EmelaString actual = cluster(text, index);
        uint16_t length = v->boundaries[index + 1] - v->boundaries[index];
        assert(actual.length == length);
        assert(memcmp(actual.data, v->bytes + v->boundaries[index], length) == 0);
        emela_embedded_release(actual.owner);
      }
      for (int32_t index = 0; index < v->point_count; ++index) assert(point(text, index) == v->points[index]);
      assert(point(text, v->point_count) == -1);
      char buffer[64];
      EmelaString actual = joined(text, buffer, sizeof buffer);
      assert(actual.length == v->length && actual.owner == NULL);
      assert(memcmp(actual.data, v->bytes, v->length) == 0);
    }
  }
  assert(cluster_count((EmelaString){NULL, 0, NULL}) == 0);
  assert(point((EmelaString){NULL, 0, NULL}, 0) == -1);
  return 0;
}
"#);
    support::compile_and_run(&output, &harness);
}

#[test]
fn discarded_unicode_lists_preserve_invalid_utf8_defects() {
    for expression in ["String.chars(text)", "String.code_points(text)"] {
        let output = output(&format!(
            "@export(c, \"invalid\")\npub fn invalid(text: String) -> () {{ {expression}\n () }}"
        ));
        support::compile_and_run(
            &output,
            r#"
#include <stdlib.h>
#include <string.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k, const char *m, uint16_t n) {
  const char expected[] = "invalid UTF-8 string";
  exit(k == EMELA_DEFECT_PANIC && n == sizeof expected - 1 && memcmp(m, expected, n) == 0 ? 0 : 3);
}
int main(void) { const char bytes[] = {(char)0xff}; invalid((EmelaString){bytes, 1, NULL}); return 2; }
"#,
        );
    }
}
