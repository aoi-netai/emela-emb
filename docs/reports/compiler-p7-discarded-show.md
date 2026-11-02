# P7 結果を捨てるShowStringの容量検査

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の `crates/emela-codegen-emb/tests/builtin_audit/mod.rs:159`、`discarded_runtime_show_string_keeps_its_capacity_check` を照合した。旧正常IRは `Value::Runtime { ShowString }` の結果を保存しなくても生成でき、String容量3でinput `abc`を表示すると容量defectになる。

独立backendでは、結果破棄時の一時出力先補完がBuiltinとListだけに適用されていた。この入力はC生成時に `RuntimeOp::ShowString has no String destination` で拒否され、旧観測を満たしていなかった。補完対象へRuntime ShowStringだけを加え、既存のString計画と結果解放へ接続した。

`backend/lower/p7_discarded_tests.rs::discarded_runtime_show_string_still_checks_its_output_capacity` は、容量3に収まる `a` の表示を1000回捨て、その後 `abc` によるStringCapacity defectを確認する。hookは意図した最後の呼出しだけ成功として扱う。変更前の拒否を再現し、変更後はC11 `-Wall -Werror -O0/-O2` のcompile/link/実行が成功した。

同時に、旧 `discarded_float_rounding_preserves_the_out_of_range_defect`（同file95行）は既存試験が全観測を含むと確認した。移植先は `tests/p2_compat.rs::legacy_numeric_formatting_rounding_and_unused_defects_execute`、入力 `compat/p2/numeric-format.emel` の `discard_floor`、C harnessのNaN/Inf/範囲外の4演算40defect検査である。旧のdiscarded Floor(NaN)とFloatToInt kindに加え、messageも確認する。新たな重複試験は増やしていない。

検証: 新IR回帰1pass、strict Clippy全target、実Rust 1.85.0 workspace全target check、fmt/diff checkが終了0。正式P5統合後のrecord update source3testもC両最適化で再確認した。正本台帳のstatusはこのcommitで変更しない。
