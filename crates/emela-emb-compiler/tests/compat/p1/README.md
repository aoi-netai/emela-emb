# P1のsource→C互換試験

基準は旧fork `c34c0aeefdd966d854371b3110db00a47798751d`、ライセンスはApache-2.0（repository rootのLICENSE）。旧frontendやcore/std bundleへ依存しない独立した入力とC mockである。

| 入力/C harness | 旧試験の観点 | 今回観測するもの |
| --- | --- | --- |
| `modules-main.emel`, `math.emel`, `modules-harness.c` | `tests/emit.rs::{compiles_direct_calls_to_internal_and_external_c_functions,compiles_if_statements_and_boolean_short_circuiting,emits_and_runs_recur_with_multiple_loop_variables}` | 複数module、internal/external呼出し、公開境界、分岐、shadowing、自己末尾再帰、引数の同時更新 |
| `numeric.emel`, `numeric-harness.c` | `tests/emit.rs` のInt32/Int64/Float primitive、signed boundary、bitwise、shift、division/remainder | wrap、signed除算と余り、MIN/-1、shift幅以上、比較、scalarのC戻り値 |
| `external.emel`, `external-harness.c` | `tests/external_scalar/mod.rs::native_external_int64_and_float_calls_preserve_bits_without_heap` | Int64 ±9007199254740993とMIN/MAX、Float ±0・subnormal・±inf・NaN payload、mixed scalar引数、1000反復 |
| `tests/p1.rs` 内のsource/C mock | `tests/emit.rs::{int32_division_by_zero_calls_the_board_defect_hook,int_shift_builtins_handle_width_and_negative_amount_boundaries,int64_shift_builtins_handle_width_and_negative_amount_boundaries}` および `tests/value_audit/mod.rs::discarded_integer_division_and_remainder_still_raise_defects_in_sync_and_suspend_code` の同期部分 | 捨てた計算結果にも残るdefect、kind/message、assert、短絡と引数評価順、位置付き拒否診断 |

C harnessの数値期待値・bit patternは上記の旧試験から抽出・再構成した。元のIR fixtureを新しいsourceに置き換え、P1以降のeffect/indirect/suspend/ISRの観点はこの抽出に含めていない。旧試験全体の完了判定は `docs/compiler-coverage.json` に残す。

`tests/support/mod.rs::compile_and_run` は生成物を保存し、全生成Cを `cc -std=c11 -Wall -Werror -O0` と `-O2` でそれぞれコンパイル・リンク・実行する。compilerや実行環境の不在をskipせず失敗にする。CLIのintegration testも、CLIが保存した生成物を同じ2設定で実行する。
