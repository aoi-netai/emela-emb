mod support;

use emela_emb_compiler::compile;
use support::compile_and_run;

#[test]
fn every_legacy_constant_string_comparison_folds_without_a_runtime_helper() {
    // Fixed baseline tests/string_comparison/mod.rs: five pairs and all six operators.
    let pairs = [
        (r#""""#, r#""""#, 0),
        (r#""a\u{0}b""#, r#""a\u{0}c""#, -1),
        (r#""ab""#, r#""a""#, 1),
        (r#""\u{e000}""#, r#""\u{10000}""#, -1),
        (r#""é""#, r#""e\u{301}""#, 1),
    ];
    let mut source = String::new();
    let mut checks = String::new();
    for (pair, (left, right, order)) in pairs.into_iter().enumerate() {
        for (op, symbol) in ["==", "!=", "<", "<=", ">", ">="].into_iter().enumerate() {
            let index = pair * 6 + op;
            source.push_str(&format!(
                "const COMPARE_{index} = {left} {symbol} {right}\n@export(c, \"compare_{index}\")\npub fn compare_{index}() -> Bool {{ COMPARE_{index} }}\n"
            ));
            let expected = match symbol {
                "==" => order == 0,
                "!=" => order != 0,
                "<" => order < 0,
                "<=" => order <= 0,
                ">" => order > 0,
                ">=" => order >= 0,
                _ => unreachable!(),
            };
            checks.push_str(&format!(
                "assert(compare_{index}() == {});\n",
                u8::from(expected)
            ));
        }
    }
    let output = compile(&source).unwrap();
    assert!(!output.c_source.contains("emela_emb_string_compare"));
    compile_and_run(
        &output,
        &format!(
            "#include <assert.h>\n#include \"emela_embedded.h\"\nint main(void) {{ {checks} return 0; }}"
        ),
    );
}
