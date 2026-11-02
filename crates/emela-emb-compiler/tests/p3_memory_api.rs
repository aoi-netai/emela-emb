mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdint.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind,const char *message,uint16_t length) {
  (void)kind;(void)message;(void)length;abort();
}
"#;
#[test]
fn strings_lists_and_trusted_bytes_pure_apis_execute() {
    let source = r#"
import Std.Bytes.{Bytes}
@export(c, "memory_api")
pub fn memory_api() -> Int {
  assert String.chars("á🇯🇵\u{0}") == ["á", "🇯🇵", "\u{0}"]
  assert String.split("a--b--", "--") == ["a", "b", ""]
  assert String.split("é", "") == ["é"]
  assert String.code_points("é\u{0}🌸") == [233, 0, 127800]
  assert String.concat(["a", "\u{0}", "b"]) == "a\u{0}b"
  assert String.join(["a", "b"], ",") == "a,b"
  assert String.to_ascii_lowercase("AZ-é") == "az-é"
  assert String.to_ascii_uppercase("az-é") == "AZ-é"
  b = Bytes.hex("00aBff")
  assert Bytes.size(b) == 3 && !Bytes.is_empty(b) && Bytes.is_empty(Bytes.empty())
  assert Bytes.to_list(b) == [0, 171, 255]
  assert Bytes.get(b, -1) == None && Bytes.get(b, 0) == Some(0) && Bytes.get(b, 3) == None
  assert Bytes.to_hex(b) == "00abff"
  assert Bytes.from_hex("00ABff") == Some(b)
  assert Bytes.from_hex("a") == None && Bytes.from_hex("a ") == None
  assert Bytes.from_list([0, 171, 255]) == Some(b)
  assert Bytes.from_list([-1]) == None && Bytes.from_list([256]) == None
  assert Bytes.slice(b, -10, 2) == Bytes.hex("00ab")
  assert Bytes.slice(b, 2, 1) == Bytes.empty()
  assert Bytes.concat([Bytes.hex("00"), Bytes.hex("abff")]) == b
  assert Bytes.append(Bytes.hex("00"), Bytes.hex("abff")) == b
  assert Bytes.decode_utf8(Bytes.encode_utf8("日本\u{0}🌸")) == Some("日本\u{0}🌸")
  assert Bytes.decode_utf8(Bytes.hex("c080")) == None
  assert Bytes.decode_utf8(Bytes.hex("eda080")) == None
  assert Bytes.decode_utf8(Bytes.hex("f4908080")) == None
  assert inspect(b) == "Bytes.hex(\"00abff\")"
  assert Ord.compare(Bytes.hex("00ff"), Bytes.hex("0100")) == Less
  assert [1,2,3].length() == 3
  assert b.to_hex() == "00abff"
  42
}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(32768),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        &format!(
            "{BOARD}\nint main(void) {{ for(int i=0;i<100;i++) assert(memory_api()==42); return 0; }}"
        ),
    );
}
#[test]
fn a_user_type_named_bytes_does_not_select_the_intrinsic_representation() {
    let output = compile("type Bytes(value: Int)\n@export(c, \"user_bytes\")\npub fn answer() -> Int { Bytes(42).value }").unwrap();
    support::compile_and_run(
        &output,
        "#include \"emela_embedded.h\"\nint main(void) { return user_bytes()==42 ? 0:1; }",
    );
    for source in [
        "@intrinsic\npub opaque type Bytes\nfn main() { () }",
        "import Std.Bytes.{Bytes}\nfn main() { Bytes }",
        "import Std.Bytes.{Bytes}\nfn main(value: Bytes[Int]) { () }",
        "fn main(value: Bytes) { () }",
        "import Std.Bytes.{Bytes}\nfn main() { Bytes.decode_valid_utf8(Bytes.empty()) }",
    ] {
        let errors = compile(source).unwrap_err();
        assert_eq!(
            errors[0].source.as_deref(),
            Some("main.emel"),
            "{source}: {errors:?}"
        );
    }
}
