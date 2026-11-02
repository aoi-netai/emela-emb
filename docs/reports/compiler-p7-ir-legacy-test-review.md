# P7 旧86件のIR試験と独立compilerの直接証拠

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の正本115試験を、保存済みsource manifestの完全な `target::test` 名と照合した。29件はsource経路、86件はIR経路である。対応案は `p7-ir-legacy-test-proposal.json` に全86件の旧path・期待する観測・新試験symbol・残件を収録する。正本台帳は変更しない。旧repoのdirty checkoutや `/tmp` の内容は、通常試験・このreportの参照先に含めない。

現在は86件すべてに受理候補を提示した。内訳は直接観測82件、表現の置換2件、Handle入力方針の置換1件、実在しないStage selector1件。関連する試験が存在するだけでは候補へ昇格させず、元の観測が不足する部分はpendingに残す。特にStringJoinの容量defectをStringAppendの容量試験で、checked_addの全境界を一つのNone観測で代用しない。

このcheckpointの新しい直接IR回帰は次のとおり。

| 独立試験 | 固定旧の期待値・追加観測 |
| --- | --- |
| `p7_misc_value_tests.rs::float_primitive_cases_keep_the_fixed_old_arithmetic_comparison_and_nan_results` | 旧Float Primのcase table全件。算術、比較、NaNのEq/Ne |
| `derived_bytes_equality_and_display_keep_unsigned_values_at_the_c_boundary` | unsigned byte 128/255のEqと `Bytes.hex("0080ff")`。1000回 |
| `dynamic_defect_preserves_an_offset_string_view_and_embedded_nul` | offset viewの長さとNULをnative/suspend両方のPanic hookまで保持 |
| `debug_print_passes_the_original_string_view_to_the_board_hook` | `abc` の3byte viewをdebug hookへ1000回渡す。不要なdefect APIは要求しない |
| `discarded_integer_division_and_remainder_keep_defects_in_native_and_suspend_callers` | I32/I64 × Div/Rem × native/suspendの8経路。結果なしでもDivisionByZero |
| `unreachable_terms_raise_the_native_or_scheduler_defect_when_reached` | 実際にnodeへ到達するnative/suspend経路。ASSERTのkindと `unreachable code` / `invalid scheduler state` の全byte |
| `p7_evidence_tests.rs::discarded_handler_and_evidence_constructors_release_before_the_next_statement` のchars分岐 | discarded StringCharsを一つの関数内で1000回繰り返し、各discard直後にlive block0を検査。関数も1000回 |

直接Unreachableだけを含む最小suspend programでは、生成Cの `fiber` 引数と共通 `emela_frame_return` が未使用となりstrict Cで失敗した。Defectのみのframeでも同じ未使用引数を確認したため、全frame stepのprologueで引数を明示消費し、return helperは既存のscheduler helperと同じ `static inline EMELA_UNUSED` とした。node到達時のdefect動作は変更しない。helper削除や戻り経路の捏造で警告を避けない。

fatal hookは各C実行で一度観測して終了する。これらを1000回の正常反復と表記しない。所有値を返す正常試験は既存のheap空検査を併用し、StringCharsの長寿命loopでは次のstatementより前の解放も検査する。

保持IRの扱いは、先行 `compiler-p7-input-and-evidence.md` と `compiler-p7-ir-contexts.md` の証拠をそのまま結ぶ。Unwind/MakeClosure/EnvGet/Dup/Drop/Function.envは実際に入力を構築し、native/suspend双方で拒否を確認する。Handleは入力に存在し、検証後Block/Breakへ正規化する合法入力である。旧Flow stageの拒否をそのまま再現する方針ではないため、正常・失敗実行と不正な型/局所変数の拒否を合わせて置換の根拠とする。N/Aは独立ProgramにないStage selectorだけである。

最後の6件は `p7_list_alias_tests.rs` の4試験へ直接追加して成功した。旧heap1024の返却List、共有tail、Unit head、Listを含むtuple/list要素と返却tupleの解放を1000回、各回heap空で検査する。NULL/nonnull empty List境界はheap0で1000回、heap0の借用List alias返却は入力拒否を確認する。captured failing closureは旧heap2048でcapture40/input2→42を1000回heap空、続くinput0→unhandled error hookを1回観測する。初版で別に残していた5件は、この追補で直接IR試験へ追加して成功した。`static_string_constant_returns_a_nul_and_unicode_view_without_an_owner`、`int64_decimal_conversion_uses_exactly_twenty_bytes_of_scratch`、`checked_add_preserves_all_four_legacy_option_boundaries`、`code_point_lists_require_a_heap_even_when_only_the_head_is_returned`、`split_join_keeps_the_old_multibyte_value_and_capacity_five_defect` が対応する。StringJoinは旧heap2048を維持し、成功1000回後のheap空と、容量5の独立fatal実行を分けて観測する。各件の期待値と証拠はJSONに残す。

rootのFiber OnFail収集・ID変換修正に対する独立IRのHandle/Break回帰も `p7_fiber_edge_tests.rs` の2試験で成功した。親arenaに未使用Var/Seq/Labelを11個ずつ挿入し、lift先のcompact arenaとのID一致で不備を隠さない。捕捉したtyped failureをAttemptでCauseへ変換し、別closure内のCauseRaiseのHandle/Breakからpayload42を回復する。heap4096・両最適化各1000回、呼出し後のheap空を確認する。source後IR seam12件は別checkpoint `88677b6` と `p7-cleanup-ir-seam-compat.json` で実行済みであり、このIR86報告には混ぜない。構造化context122件はfrontend担当が証拠照合を進め、Stage5件はrootが表現上の差を判定する。この初版をそれらの完了報告には使わない。共通系99contextsはfrontend担当と分担しており、先行context proposalの実数は15件である。

検証はcompiler crate内の通常unit testからCを生成し、共通helperがC11 `-Wall -Wextra -Werror -O0/-O2` の双方をcompile/link/実行する。compiler不在時のskipはない。focused misc11件・Fiber edge2件・List/captured failure4件、backend115件、workspace全target strict Clippy、実Rust1.85.0全target check、fmt/diffをcheckpoint時点で確認する。
