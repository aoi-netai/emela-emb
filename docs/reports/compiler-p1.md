# P1 source・CLI・C実行の検証報告

基準旧fork: `c34c0aeefdd966d854371b3110db00a47798751d`。この報告はP1の証拠であり、P2〜P7および旧115試験全体の完了を表明しない。正本の判定状態は [移行台帳](../compiler-coverage.json) に保持する。

## 実行環境と再現

- 検証したsource revision: `2444fd779736baa8077433573ff0ccbff619745f`（workspace全試験）。追加の非有限Float独自IR fixtureのみを `a4d252bc16780af8f6f5ab8d6d647ec6ebbf6e30` で検証した。後者の差分は試験1件の追加のみ。未コミット変更はこの報告、fixture出自README、最小実験の履歴案内だけで、検証対象のcode/test差分はない。
- OS: WSL2 Linux `6.18.54.1-microsoft-standard-WSL2`、target `x86_64-unknown-linux-gnu`。
- Rust実体: `/home/aoi_nemui/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/{rustc,cargo}`。C実体: `/usr/bin/x86_64-linux-gnu-gcc-11`。
- 事前に確認したtoolchain: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`、`cargo 1.99.1 (5f94df478 2026-08-27)`、`cc (Ubuntu 11.4.0-1ubuntu1~22.04.3) 11.4.0`。
- 必須C compilerは`cc`。`tests/support/mod.rs` とCLIのintegration testは `-std=c11 -Wall -Werror -O0` / `-O2` で別々に全生成Cをコンパイル・リンク・実行する。`cc`不在、コンパイル失敗、実行失敗をskipしない。
- C mockは `tests/compat/p1/*-harness.c` および `tests/p1.rs` 内に固定したもののみ。通常の試験経路は旧checkout・upstream crate・CLI・core/std bundleを参照しない。

入力拒否試験は生成Cが存在しないためC実行を適用しない。最終統合HEAD・独立環境の結果は末尾へ追記する。

## P1のsource/API・CLI観測

| 新試験 | 観測する契約 |
| --- | --- |
| `tests/p1.rs::modules_internal_calls_branches_and_tail_recursion_execute` | 2 module、公開/内部境界、direct external、shadowing、assert、if、100万回tail recur、3引数の同時入替え |
| `scalar_numeric_boundaries_execute_with_legacy_semantics` | Int/Int64のwrap・MIN/MAX・除算/余り・bitwise/shift・相互変換、Float.from_int/演算とNaN比較、Bool、Unit |
| `native_scalar_externals_preserve_integer_and_float_bits` | Int64 MIN/MAX/±9007199254740993、Float ±0/subnormal/±inf/NaN payloadを1000反復、mixed scalar引数、heapなし |
| `short_circuiting_and_argument_evaluation_preserve_side_effect_order` | 左から右への引数評価、&&/||の短絡、呼出し回数 |
| `unused_division_remainder_and_negative_shifts_still_raise_defects` | 未使用のInt/Int64 div/rem/3種類のshiftでもkind/messageを保ったdefect停止 |
| `failed_source_assertion_calls_the_board_defect_hook_without_continuing` | source assertをlowerしdefect hookへ渡す。後続externalを実行しない |
| `entry_source_and_program_entry_are_selected_explicitly` | entry sourceとprogram関数の選択、Optionsの明示入力、pub-only lifecycle入口はC exportへ公開しない |
| `never_returning_external_preserves_the_unreachable_contract` | Never externalをNever/Int戻りのexportから呼び、停止後に継続しない |
| `only_explicit_c_exports_cross_module_public_boundaries` | import先の明示@export C symbolを実行し、pub-only関数はheaderへ公開しない |
| `diagnostics_identify_source_and_precise_invalid_expression`, `private_imports_unknown_modules_and_unknown_entry_sources_are_rejected` | 未定義名、型、arity、export衝突、private import、module/entry不在の位置付き拒否 |
| CLI `tests/build.rs` | 複数入力/entry/Options、全生成物保存、CLI保存物のC実行、import先の位置診断、atomic保存・既存出力/symlink保護 |

source/C fixtureの旧試験からの抽出範囲と出自は `crates/emela-emb-compiler/tests/compat/p1/README.md` を参照する。

## 旧115関連試験のP1照合

全115キーを既存台帳から照合した。以下はP1に関係する抽出・対応候補であり、旧試験の全観点を検証したという判定ではない。特にsource経路の成功から旧IR入力経路、suspend経路、bundled標準module、ISRの成功を推定しない。

| 旧試験名 | 新試験候補（`tests/p1.rs`、別記を除く） | 抽出範囲と残観点 |
| --- | --- | --- |
| `compiles_direct_calls_to_internal_and_external_c_functions` | `modules_internal_calls_branches_and_tail_recursion_execute` | internal/external scalar direct call |
| `compiles_if_statements_and_boolean_short_circuiting` | `modules_internal_calls_branches_and_tail_recursion_execute; short_circuiting_and_argument_evaluation_preserve_side_effect_order` | if、Bool短絡、評価順 |
| `emits_and_runs_a_block_that_yields_a_value` | `modules_internal_calls_branches_and_tail_recursion_execute` | source block/ifの値。独自IRのBlock/Yieldはbackend試験も参照 |
| `emits_and_runs_a_loop_with_an_initial_value_and_yield` | `modules_internal_calls_branches_and_tail_recursion_execute` | source自己末尾再帰。独自IRのLoop/Yieldはbackend試験も参照 |
| `emits_and_runs_recur_with_multiple_loop_variables` | `modules_internal_calls_branches_and_tail_recursion_execute` | 3引数tail recurと入替えの同時更新 |
| `emits_and_runs_break_to_an_enclosing_block` | `backend::lower::tests::block_yield_and_break_to_enclosing_block_execute` | 独自IR Block/Yield/Breakの戻り値をC実行で観測 |
| `emits_and_runs_int32_primitive_operations` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | wrap、Bool not、比較 |
| `emits_and_runs_int64_constants_at_the_signed_boundaries` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | Int64.MIN/MAXをsourceからCで観測 |
| `emits_and_runs_float_constants_including_non_finite_values` | `backend::lower::tests::float_constant_bits_and_non_finite_values_execute` | 独自IR Const::Float 1.25/1e100/-0/±inf/NaNをC isinf/isnan/signbitで観測 |
| `emits_and_runs_float_primitive_operations` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | 四則/余り/負号、全比較、NaNの比較 |
| `emits_c_for_a_function_returning_an_int_constant` | `modules_internal_calls_branches_and_tail_recursion_execute; tests/compile.rs` | 定数、公開名、決定性、生成物の整合性 |
| `emits_int32_division_and_remainder_with_emela_semantics` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | -7/3、-7%3、MIN/-1 |
| `int32_division_by_zero_calls_the_board_defect_hook` | `unused_division_remainder_and_negative_shifts_still_raise_defects` | kind/message、捨てた結果にもdefect |
| `float_from_int_converts_the_full_signed_range_exactly` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | Int32 MIN/MAX/-1の正確なFloat変換 |
| `int64_conversions_preserve_sign_and_keep_the_low_32_bits` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | signed拡張、低32bit、Int64 MIN/MAX/0x80000000 |
| `int64_bitwise_builtins_match_signed_64_bit_semantics` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | and/or/xor/not |
| `int_bitwise_builtins_match_signed_32_bit_semantics` | `scalar_numeric_boundaries_execute_with_legacy_semantics` | and/or/xor/not |
| `int64_shift_builtins_handle_width_and_negative_amount_boundaries` | `scalar_numeric_boundaries_execute_with_legacy_semantics; unused_division_remainder_and_negative_shifts_still_raise_defects` | 幅以上、算術/論理右shift、負数shift defect |
| `int_shift_builtins_handle_width_and_negative_amount_boundaries` | `scalar_numeric_boundaries_execute_with_legacy_semantics; unused_division_remainder_and_negative_shifts_still_raise_defects` | 幅以上、算術/論理右shift、負数shift defect |
| `value_audit::discarded_integer_division_and_remainder_still_raise_defects_in_sync_and_suspend_code` | `unused_division_remainder_and_negative_shifts_still_raise_defects` | 同期Int/Int64のみ抽出。suspend経路はP5以降 |
| `binding_metadata::imported_c_bindings_preserve_symbols_for_named_file_and_bundled_modules` | `modules_internal_calls_branches_and_tail_recursion_execute; native_scalar_externals_preserve_integer_and_float_bits` | 通常のfile/module externalを抽出。bundled moduleは後段 |
| `rejects_source_builtins_that_must_lower_to_flow_control_or_runtime` | `failed_source_assertion_calls_the_board_defect_hook_without_continuing` | assertのsource loweringのみ。Dbg等と旧未lower入力契約は後段 |
| `lowers_and_verifies_core_as_flow_ir` | `backend cfg(test) fixture; diagnostics_identify_source_and_precise_invalid_expression` | 独自IR検証のみ対応先。旧Core/Flow入力APIは採用しない |

旧115件の各`legacy_path`を列挙し、上表のP1候補と残りの件数を照合する。残りは未実行であり、P2〜P7で元fixture全体を接続する。`flow`の旧入力拒否は対象外の即断をせず、独自IRの検証と意味の保存を個別に確認する。

| 固定旧path | 関連試験数 | P1対応候補数 | 上表以外（未実行） |
| --- | ---: | ---: | ---: |
| `src/compiler/tests/separation.rs` | 3 | 0 | 3 |
| `tests/binding_metadata/mod.rs` | 2 | 1 | 1 |
| `tests/builtin_audit/mod.rs` | 3 | 0 | 3 |
| `tests/builtin_audit/retire.rs` | 1 | 0 | 1 |
| `tests/bytes/mod.rs` | 13 | 0 | 13 |
| `tests/cell/mod.rs` | 3 | 0 | 3 |
| `tests/completion_types/mod.rs` | 1 | 0 | 1 |
| `tests/const_runtime/mod.rs` | 1 | 0 | 1 |
| `tests/deferred/mod.rs` | 4 | 0 | 4 |
| `tests/emit.rs` | 53 | 19 | 34 |
| `tests/fiber/aggregate_abi.rs` | 1 | 0 | 1 |
| `tests/fiber/attempt.rs` | 1 | 0 | 1 |
| `tests/fiber/await_.rs` | 1 | 0 | 1 |
| `tests/fiber/cause.rs` | 4 | 0 | 4 |
| `tests/fiber/failure.rs` | 1 | 0 | 1 |
| `tests/fiber/function_aggregate.rs` | 2 | 0 | 2 |
| `tests/fiber/interrupt.rs` | 1 | 0 | 1 |
| `tests/fiber/join.rs` | 2 | 0 | 2 |
| `tests/fiber/protect.rs` | 1 | 0 | 1 |
| `tests/flow.rs` | 8 | 2 | 6 |
| `tests/string_append/mod.rs` | 1 | 0 | 1 |
| `tests/string_comparison/mod.rs` | 1 | 0 | 1 |
| `tests/suspend/mod.rs` | 4 | 0 | 4 |
| `tests/value_audit/discarded.rs` | 2 | 0 | 2 |
| `tests/value_audit/mod.rs` | 1 | 1 | 0 |
| 合計 | 115 | 23 | 92 |

旧115関連試験に含まれない追加観点もP1で抽出した。`tests/external_scalar/mod.rs` のnative bit維持、`tests/exports.rs` の公開metadata、`tests/emit.rs::emits_and_runs_int64_primitive_operations_with_wrapping_semantics` をsource/C試験へ接続する。external_scalarの間接呼出し・handler・ISR全体とexportsのsuspend入口は後段の残件である。

## 固定旧source corpusの探索範囲

基準commitの `crates/emela-codegen-emb/` tracked treeで、`.emel`ファイルは38件（examples 27、lib 6、tests 5）。加えてRust試験に埋め込まれたsource文字列と独自IR fixtureがあるため、この38件を旧source全体と数えない。P1のscalar抽出はRust試験の観点を新sourceへ変換したものである。下記fixtureの未対応宣言を無視して正常終了させる方法は採らない。

| 固定旧source path | 依存する主な機能（全体実行はP1未完了） |
| --- | --- |
| `examples/support/aggregate_suspend.emel` | String, effect, handler, Fiber, Async |
| `examples/support/const_polymorphic.emel` | String, List, const, effect, handler, Fiber, enum, error, fails |
| `examples/support/const_polymorphic_dependency.emel` | String, List, const |
| `examples/support/const_runtime.emel` | String, const |
| `examples/support/deferred.emel` | Fiber, Scope, Deferred, Async |
| `examples/support/deferred_aggregate.emel` | String, List, Fiber, Scope, Deferred, Async |
| `examples/support/error_payload_native.emel` | String, List, effect, handler, error, fails |
| `examples/support/external_task_failure.emel` | String, List, effect, handler, Fiber, error, Async, fails |
| `examples/support/external_task_values.emel` | String, List, effect, handler, Fiber, Scope, Deferred, enum, Async |
| `examples/support/fiber_aggregate_abi.emel` | String, List, effect, handler, Fiber, enum, Async |
| `examples/support/fiber_aggregate_failure.emel` | String, Fiber, error, Async, fails |
| `examples/support/fiber_child_failure_reclaim.emel` | String, effect, handler, Fiber, error, Async |
| `examples/support/fiber_child_reclaim.emel` | String, List, effect, handler, Fiber, Scope, Async |
| `examples/support/fiber_failure.emel` | String, effect, handler, Fiber, error, Async |
| `examples/support/fiber_interrupt.emel` | effect, handler, Fiber, Async |
| `examples/support/fiber_join.emel` | Fiber, Async |
| `examples/support/fiber_notes.emel` | String, effect, handler, Fiber, error, Async, fails |
| `examples/support/fiber_payload.emel` | String, effect, handler, Fiber, error, Async, fails |
| `examples/support/fiber_scope.emel` | effect, handler, Fiber, Async |
| `examples/support/function_aggregate.emel` | String, List, enum |
| `examples/support/owned_payload.emel` | String, List |
| `examples/support/phase7_integration.emel` | String, List, effect, handler, Fiber, Scope, Deferred, error, Async, fails |
| `examples/support/scope_failure_abi.emel` | Fiber, error, Async |
| `examples/support/shared_cell.emel` | String, List, effect, handler, Fiber, enum, error, Async |
| `examples/support/string_comparison.emel` | String |
| `examples/support/string_comparison_suspend.emel` | String, effect, handler, Async |
| `examples/support/suspend_handler.emel` | effect, handler |
| `lib/embedded/board.emel` | String, List, handler, error, fails |
| `lib/embedded/gpio.emel` | effect, type, enum |
| `lib/embedded/i2c.emel` | List, effect, type, enum, fails |
| `lib/embedded/peripheral.emel` | error |
| `lib/embedded/spi.emel` | List, effect, type, enum, fails |
| `lib/embedded/uart.emel` | List, effect, type, enum, fails |
| `tests/bytes/fixtures/bytes.emel` | String, List, Bytes, const, type |
| `tests/embedded_peripherals/exercise.emel` | String, List, error, Async |
| `tests/release_0_24/collections.emel` | List, type |
| `tests/release_0_24/helpers.emel` | String, List, type, enum, fails |
| `tests/release_0_24/traits.emel` | String, Fiber, type, enum, error, trait, fails |

P1で検証した同期scalar部分を理由に、上記の混在fixture全体や横断要件を`verified`へ変更しない。P2以降はこの一覧に加え、Rust中のsource文字列とfixture依存先も正本台帳へ接続する。

## validation worktreeでの実行結果

| command | source revision | 結果/終了コード |
| --- | --- | --- |
| `cargo test --workspace --locked` | `2444fd7` | CLI unit 3、CLI integration 17、compiler lib 20、compile 4、frontend scalar 7、P1 integration 11、計62試験pass。失敗0、ignored/skip 0。終了0 |
| `cargo test -p emela-emb-compiler --lib float_constant_bits_and_non_finite_values_execute --locked` | `a4d252b` | 追加IR試験1件pass、失敗0、ignored/skip 0。終了0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | `a4d252b` | pass、終了0 |
| `cargo fmt --all --check` | `a4d252b` | pass、終了0 |

P1 integrationの正常source 9試験中、未使用defectは10個の入力（Int/Int64各div/rem/3種類のshift）、Neverは2種類の公開署名でCを実行した。いずれもhelperが `-O0` と `-O2` を順に実行して両方passした。CLIの保存物C実行1試験、backendのLoop/Recur/Block/Break/未使用defect/Float定数も両最適化でpassした。位置付き拒否2試験はC生成前の診断を確認し、C実行は適用しない。旧heap unit試験はcc必須でskip経路がないことも確認したが、最適化指定がないためP1の両最適化の証拠は新helper/新IR試験を使用する。

P1対象に失敗・skip・未実行は残っていない。旧115件全体、38 source fixture全体、P2以降の機能は前節の残観点として残る。台帳のsource_corpus/numeric_unicode等の横断要件全体をこの結果だけでverifiedへ進めない。

## 最終統合の追記欄

rootで最終HEAD、固定台帳の構造監査、workspace全checks、本体checkoutを参照できない独立環境の結果を追記する。

