# P2同期機能の検証報告

基準旧fork: `c34c0aeefdd966d854371b3110db00a47798751d`。P2の対応状態は [移行台帳](../compiler-coverage.json) を正本とする。ここでは機能別にsource入力・C mock・拒否入力を記録する。通常の試験経路で本体checkout・CLI・core/std bundleを参照しない。

## String・Unicode・数値整形の先行検証

入力は `crates/emela-emb-compiler/tests/compat/p2/`、試験は `tests/p2_compat.rs`、C実行はP1から共通の `tests/support/mod.rs::compile_and_run` を使用する。全生成Cを `cc -std=c11 -Wall -Werror -O0` / `-O2` で個別にコンパイル・リンク・実行する。C compiler不在と失敗はskipしない。

| 新試験 | 観測した値・副作用・拒否 |
| --- | --- |
| `legacy_string_views_comparisons_append_and_bounded_returns_execute` | NUL、非ASCII、空view、65535byte viewの全6比較、100万回String引数入替えtail recur、literal prefix append、caller buffer/ownerless戻り、contains/prefix/suffix/trim |
| `source_string_length_matches_every_unicode_18_grapheme_vector` | 固定Unicode18公式853 vectorのcluster数とbyte数を、source String.length/byte_sizeから生成したCで比較。StringCharsの分割ListはP3以降の別観点 |
| `malformed_utf8_string_length_raises_the_legacy_defect` | 不正UTF-8 viewのPanic kindと`invalid UTF-8 string` message |
| `legacy_numeric_formatting_rounding_and_unused_defects_execute` | Int/Int64 MIN/MAX/0の10進表記、Floatの最短表記/±inf/NaN、floor/ceil/round/truncateのInt境界、NaN/±inf/範囲外と未使用結果の40 defectをkind/message込みで観測 |
| CLI `cli_string_capacity_and_caller_output_buffers_execute` | CLIの実保存物をC実行し、NUL含む戻り値・caller buffer・`--string-capacity 32` の超過defectを確認 |

固定旧試験からC値・期待値を抽出し、新しい明示`@export`付きsourceへ再構成した。出自はApache-2.0、LICENSEはrepository root。旧frontend/IR型は持ち込まない。

| 固定旧試験・fixture | 新fixture/試験への対応 | 今回の抽出に含まない観点 |
| --- | --- | --- |
| `tests/string_append/mod.rs::literal_prefixes_are_copied_without_treating_readonly_bytes_as_writable_output` | `strings.emel`, `append.emel`, `strings-harness.c` とString/CLI試験 | なし。生成Cの整形・変数名を比較せず、readonly入力と結果をCで比較 |
| `examples/support/string_comparison.emel` / `tests/string_comparison/mod.rs::source_comparisons_are_heapless_and_allow_isr_and_constant_space_tail_loops` | `strings.emel`, `strings-harness.c` の全比較/長view/tail recur | ISR指定と到達性はP7で別途確認 |
| `tests/string_comparison/mod.rs::all_constant_comparisons_match_runtime_order_without_emitting_the_helper` | 動的view比較は接続済み | constで静的評価されhelperを省略する全観点はconst実装後に追加 |
| `tests/emit.rs::string_length_uses_unicode_18_extended_grapheme_clusters` | `unicode.emel` とUnicode/invalidUTF8試験 | なし。公式vectorからのcluster数も追加 |
| `tests/emit.rs::string_chars_matches_the_unicode_18_grapheme_break_test` | 公式vectorのcluster数のみString.lengthへ抽出 | 元のStringChars/List/Joinと各cluster境界byte列の全比較はP3以降 |
| `tests/emit.rs::string_byte_size_counts_utf8_bytes_from_a_view`, `string_search_predicates_match_utf8_substrings_without_copying` | String/Unicode試験 | なし。viewの所有化・heapを使う組合せは別機能 |
| `tests/emit.rs::int64_to_string_uses_twenty_byte_scratch_and_matches_decimal_output` | `numeric-format.emel`, `numeric-format-harness.c` | constの静的評価とscratch省略は後続 |
| `tests/emit.rs::dynamic_float_formatting_uses_a_bounded_runtime_and_float_parse_uses_the_bounded_parser` | 動的Float整形の期待値/長さを接続 | Float.parse/長いdecimal/丸めtieはparse実装後に追加 |
| `tests/emit.rs::float_rounding_builtins_match_spec_and_defect_out_of_range`, `tests/builtin_audit/mod.rs::discarded_float_rounding_preserves_the_out_of_range_defect` | 数値整形試験の正常値と40 defect | const静的評価は後続 |

String引数を持つ自己tail recurで、loop cleanupが未宣言の所有flagを参照してCコンパイルが失敗する問題をこの抽出で発見した。backend `9c3a1d6` がmanaged parameterのloop所有flagを初期化・解放し、同じsource/C試験が両最適化で成功した。旧正常sourceを除外して回避していない。

先行実行環境はP1と同じWSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、cc/GCC 11.4.0。code基点はbackend `06d5397`/`1a39f4c`/`9c3a1d6` とfrontend `e60e051`。検証worktreeではそれぞれ `09339ff`/`cfe963b`/`695a7ca`/`e407f98` として採用した。

- `cargo test -p emela-emb-compiler --test p2_compat --locked`: 4 pass、失敗/skip 0、終了0。各C試験は`-O0`/`-O2`両方pass。
- `cargo test -p emela-emb-cli --test build cli_string_capacity_and_caller_output_buffers_execute --locked`: 1 pass、失敗/skip 0、終了0、保存Cを両最適化で実行。
- `cargo fmt --all --check`: pass。
- Clippyで検出したfrontend test module後の`parse_string`配置はfrontend `a6a8628` で修正され、data初回 `055393f` を含む統合後 `cargo clippy --workspace --all-targets --locked -- -D warnings` は終了0。

## 固定標準APIの照合

固定baselineの `crates/emela-core/lib/core/` と `lib/std/` から、必要な宣言・純粋bodyだけをcompiler所有libへ供給する。本体への通常/dev依存は追加しない。互換APIの存在だけを検証済みとはしない。

| 固定module | P2で必要な契約 | 後段の依存 |
| --- | --- | --- |
| Prelude | Bool、Option（Some→Noneの順）、Ordering（Less/Equal/Greater）、Eq/Ord/Show primitive/tuple、ParseError(input:String)、panic/todo/dbg/inspect | dbgの任意closure表示等はP4以降の値型にも依存 |
| Option | `or`, `is_some`, `is_none`, `or_fail`、Eq/Ord（None<Some）/Show（Some(String)は引用） | `map`/`and_then` はcallback引数があるためP4 |
| Int / Int64 / Float / Bool | parseは公開sourceで`fails ParseError`（private parse_or_noneがOptionを返す）、checked_add、数値変換/表記、Float.compare/min/maxのNaN total-orderとsigned zero | constでの静的評価も別途確認 |
| String | length/byte_size/append/contains/starts_with/ends_with/trim、from_code_point→Option[String]、show_nested | split/chars/code_points/concat/joinはList依存。ASCII caseの旧bodyはList/内部closureにも依存 |
| List | P3 data API: length/is_empty/get/head/tail/reverse/append/join/flatten/range/zip/take/drop/last/enumerate/unzip/sort、Eq/Ord/Show | map/filter/fold/partition/any/all/find/flat_map/filter_map/find_index/sort_by等のcallback APIはP4 |
| Std.Map / Std.Set | P3 data API: empty/from_list/insert/remove/get/contains/size/is_empty/to_list/keys/values/union（Setは対応する集合操作）、Eq/Show | update/fold/map_values/filter等はcallback API。旧data API内部のfold/map lambdaは段階に合わせた実装適応が必要 |
| Std.Bytes | P3: empty/from_list/to_list/size/is_empty/get/slice/append/concat/encode_utf8/decode_utf8/to_hex/from_hex/hex、Eq/Ord/Show | 所有buffer・List・再帰処理と併せて検証 |

この照合は宣言・依存の調査であり、未接続APIの完了証拠ではない。古いIRのBuiltin IntParse/FloatParseがOptionを返すことと、公開source APIの`fails ParseError`を混同しない。top-level fnのparam型省略は旧specの拒否対象で、正常な汎化sourceは `fn identity[A](value:A)->A` を使う。未呼出generic宣言の不正bodyも検査対象に保つ。

## P2の残りの確認

データ型/pattern/match、型推論・具体化・trait、fails/fail/escape、mono const、数値parseを後続の機能別検証へ接続する。旧`release_0_24/traits.emel`は同期部分を独立fixtureへ抽出し、List/closure/Fiber/Causeを含む元fixture全体の実行はP6以降に残す。先行String成功だけで旧混在fixture・旧115関連試験・横断要件全体をverifiedにしない。

## 最終統合の追記欄

P2完了時にsource revision、OS/Rust/C compiler実体、全command/結果、各最適化、拒否試験、台帳対応、独立環境の結果を追記する。
