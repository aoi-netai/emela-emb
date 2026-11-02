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
| Int / Float | parseは公開sourceで`fails ParseError`（private parse_or_noneがOptionを返す）、Int.checked_add、数値変換/表記、Float.compare/min/maxのNaN total-orderとsigned zero | constでの静的評価も別途確認 |
| Int64 / Bool | Int64は数値変換/表記・bit操作・min/max/abs。Bool.parseは文字列matchと`fails ParseError`の純粋body | 固定版Int64にparse APIはない |
| String | length/byte_size/append/contains/starts_with/ends_with/trim、from_code_point→Option[String]、show_nested | split/chars/code_points/concat/joinはList依存。ASCII caseの旧bodyはList/内部closureにも依存 |
| List | P3 data API: length/is_empty/get/head/tail/reverse/append/join/flatten/range/zip/take/drop/last/enumerate/unzip/sort、Eq/Ord/Show | map/filter/fold/partition/any/all/find/flat_map/filter_map/find_index/sort_by等のcallback APIはP4 |
| Std.Map / Std.Set | P3 data API: empty/from_list/insert/remove/get/contains/size/is_empty/to_list/keys/values/union（Setは対応する集合操作）、Eq/Show | update/fold/map_values/filter等はcallback API。旧data API内部のfold/map lambdaは段階に合わせた実装適応が必要 |
| Std.Bytes | P3: empty/from_list/to_list/size/is_empty/get/slice/append/concat/encode_utf8/decode_utf8/to_hex/from_hex/hex、Eq/Ord/Show | 所有buffer・List・再帰処理と併せて検証 |

この照合は宣言・依存の調査であり、未接続APIの完了証拠ではない。古いIRのBuiltin IntParse/FloatParseがOptionを返すことと、公開source APIの`fails ParseError`を混同しない。top-level fnのparam型省略は旧specの拒否対象で、正常な汎化sourceは `fn identity[A](value:A)->A` を使う。未呼出generic宣言の不正bodyも検査対象に保つ。

## データ型・pattern・generic関数のsource接続

`tests/p2_data.rs` でtuple、type/enum/error/opaque、constructorの型引数推論、名前付きfield、irrefutableな束縛、後置match、guardと網羅性検査を実行する。opaqueのmodule外構築・fieldアクセス、不正なpatternと網羅性不足を拒否する。

`2a6ec0e` では明示型パラメーターを持つgeneric関数と、相互参照する関数の戻り型推論を接続した。同じidentityのInt/String呼出し、generic Boxの構築・destructure、Int64の文脈によるliteral型推論、tupleの型別具体化、Stringを保持する10万回のgeneric自己末尾再帰、even/oddとforward参照をsourceからCの両最適化で確認した。無制約型パラメーターの数値演算・不一致body・generic C exportは拒否する。

- `cargo test --workspace --locked --offline`: 120 pass、失敗/skip 0、終了0（`2a6ec0e` のcode tree）。
- `cargo fmt --all --check` と `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。
- trait registryと独立型solverの単体試験も含むが、traitのsource接続完了とは数えない。未確定戻り型の自動汎化は引き続き実装対象。

## 単相constとtrait具体化の先行統合

`264410a` の `tests/p2_consts.rs` は3 source/C試験を両最適化で通過した。generic helperを含む静的const、runtime constの初期化順・一度だけの初期化・未使用constの非初期化、division defectと自己参照defectのcache、値構文と位置付き拒否を観測する。constは単一の元宣言IDを保持したinitializerへ展開し、値参照をその呼出しへ接続する。静的評価により呼ばれなくなったinitializer専用helperがCに残る問題は `859503a` で修正し、IR/C回帰も追加した。

`838d1c3` のtrait具体化は、構造の各fieldをregistry経由の通常関数呼出しへ展開する。custom Eq/Ord/Showを持つKeyをEnvelope・enum・Some・tupleへ入れた場合と、default methodからcustom methodを呼ぶ場合をtyped IR/Cで観測した。`frontend::specialize` の8試験とClippyが通過。trait/impl宣言のsource接続自体は残る。

const導入後にも実際のRust 1.85.0で `cargo check --workspace --all-targets --locked --offline` が終了0。準備済みのFail/Escape ASTをcheckerへ接続する途中のため、その未読field警告は残る。警告を抑制してP2完了とはせず、接続後の最終Clippyで解消を確認する。

## 失敗集合・fail・escapeのsource接続

`32bc50b` で値型と別のerror集合を推論し、fail/escapeをbackendの失敗辺へ接続した。明示`-> T`でfails省略なら未回復の失敗を拒否し、戻り型省略では相互参照する関数の失敗集合を推論する。escapeはunguardedなpattern群がerror payloadを共同で網羅した場合にだけ、そのerrorを集合から除く。genericなerror引数は回復済みの場合も単一error型に制約し、constの未回復failを拒否する。

`tests/p2_failures.rs` の3 source試験で、引数途中の失敗による後続処理の停止、escape全体からの脱出、handler内の再失敗の外側への伝播、Bool payloadの共同網羅、回復済みconstのcache、generic or_fail、失敗集合の相互再帰推論を確認した。

- `cargo test --workspace --locked --offline`: 141 pass、失敗/skip 0、終了0。C試験は`-O0`/`-O2`とも成功。
- `cargo fmt --all --check` と `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。const導入時に残っていたAST未読警告も実装の接続で解消した。

## 戻り型の自動汎化と相互再帰

`db9cc4c` で関数呼出しの依存を追跡し、相互再帰するcomponentの全bodyを検査してから戻り型の未確定変数を量化する。明示型パラメーターの後ろに推論したパラメーターを置き、呼出しごとに新しい変数を割り当てる。component内の再帰呼出しは共有して推論し、正規化時に関数ごとの型引数へ確定する。単相constは共有の環境に残し、自動汎化しない。

`tests/p2_inference.rs` の4試験は、`None`を返す関数のInt/String別instance、forward wrapper、明示/暗黙型引数の併用、3関数の相互再帰、型引数の個数と順番が異なる相互再帰をsourceからCの両最適化で実行する。再帰先の戻り型がまだ不明な場合は、その型が判明した後に明示型引数を代入する。数値の既定型、rigidな宣言型、未使用の不正body、汎化関数のC公開拒否も検査した。

`41d161b` は、量化されずbody内だけに残った未確定型をUnit表現へ確定する。固定旧forkの`emela-core/src/lower.rs::ty`も未使用`None`等の型変数をUnitへ下げる。`unused = None`と、要素型を観測しない`is_empty(empty())`を追加し、全145試験とClippyの成功を再確認した。

P1時点の`fn f() { f() }`拒否は、戻り型の自動汎化に伴って更新した。固定旧forkの`emela-types/src/body.rs::finish`と`infer.rs::generalize_bounded`が未確定の戻り型も量化する設計に合わせ、Int/Stringの具体wrapperから生成・Cコンパイルする。停止しない関数自体は実行せず、同じ生成物の正常なexportを実行する。これを停止性の保証とは数えない。

- `cargo test --workspace --locked --offline`: 145 pass、失敗/skip 0、終了0。
- `cargo fmt --all --check` と `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。
- `cargo test -p emela-emb-compiler --release --test p2_inference --locked --offline`: 4 pass、失敗/skip 0、終了0。Rustのrelease buildでも同じC `-O0`/`-O2`試験が成功。

## Cの入口がない入力の到達性

`00de1e9` で、`@export`と選択entryのどちらもない場合に全関数を生成するfallbackを除去した。公開範囲を示す`pub`だけではC生成の入口にしない。標準ライブラリの追加により、未使用のparse等の失敗payloadがheapを要求していた問題も解消する。

`tests/p2_reachability.rs` は、未使用external・失敗payloadを含むライブラリsourceをheap 0で生成し、関数・external参照・heapを生成しないCを`-O0`/`-O2`でコンパイル・実行した。未使用bodyの型エラーは引き続き拒否する。全146試験とfmt/Clippyが成功し、IR試験のroot指定を変える必要はなかった。

## trait・標準source・const patternの統合

`cf9b746` でtrait/implの収集、署名・Self・制約の検査、method解決を具体化へ接続した。`tests/p2_traits.rs` はcustom Eq/Ord/Showを持つfield、tuple/Option/enumのderive、defaultとgeneric関数、失敗を返すmethodとescapeをsourceからCで観測する。不正なimpl、supertrait循環・未充足、重複derive、FloatのOrd、pure署名に漏れる失敗は拒否する。別moduleの同名traitはmodule IDと名前の組で分離する。

固定旧sourceのPrelude/OptionとInt/Int64/Float/Bool、Stringの同期APIをcompiler所有の標準sourceとして登録した。`tests/p2_standard_api.rs` と `tests/p2_parse.rs` は公開APIから数値境界、FloatのNaN順序・signed zero、長いdecimalと丸めtie、ParseErrorのinputを確認する。Floatの`-0.0`の文字列は固定旧実装と同じ`"0.0"`であり、min/maxの符号はCでbit比較する。parse失敗のString payloadには固定heapを設定し、C mockへの複合値受渡しは明示したborrowed adapterを使う。

`tests/p2_const_patterns.rs` はliteralによる網羅性とruntime constの一度初期化、custom Eq、Neverのconst patternで後続処理が実行されないことを確認する。`tests/p2_debug.rs` は入力を一度だけ評価し、custom Showを優先して未実装のfieldだけを再帰表示し、元の値を返すdbgをC hookで確認する。inspectには引き続きShow制約を要求する。

`62254f7` は相互再帰内で一部の関数だけが汎化する変数を、各関数の署名の範囲に正規化する。`tests/p2_inference.rs` の5件目で、Intを返す関数がgenericなNoneの戻り値を破棄する場合に他関数の型変数を漏らさないことを確認した。

- 検証したcode revision: `cf9b746`。OS/target・C compilerは前述のWSL2 x86_64/GCC 11.4、通常Rustは1.99.1。
- `cargo test --workspace --locked --offline`: 172 pass、失敗/skip 0、終了0。全C実行試験は`-O0`/`-O2`両方pass。
- `cargo fmt --all --check` と `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。
- 実際のRust 1.85.0で `cargo check --workspace --all-targets --locked --offline`: 終了0。通常のtoolchain設定を変更せず、cargo/rustc/rustdocを明示して実行した。

## P2の残りの確認

`5db890c` でmethod呼出しの省略記法と未使用の型宣言のfield制約を接続した。`tests/p2_methods.rs` はreceiverの一度評価と名前付き引数の評価順、定義moduleの公開関数、可視traitと型パラメーターのboundからの解決、曖昧なmethodとfield名衝突の拒否を確認する。`4aabd7a` の `tests/p2_modules.rs` は選択importでもmodule名を導入し、同一定義の再importを許可しつつ別定義・private名を拒否する。固定旧仕様 `spec/language/04-modules.md` のnamespace規則に合わせた。

`e445dc4` の `tests/p2_regressions.rs` は旧同期traitのcustom/default/supertraitと失敗、同名traitのmodule別dispatch、String失敗payloadの寿命、runtime constの初期化順・1000回のdefect cache、Option.or_failの引数副作用、FloatのNaN payloadとsigned zero、Unicode scalar境界を追加検証する。全経路でfailするString関数の出力容量localも、`afa12e4` でCの未使用変数警告を出さなくなった。

`af651d2` のconst評価はIf/SwitchのYield/Returnとpayloadのないenumのtagを扱い、構造を持つ値や未対応controlはruntime評価へ残す。`tests/p2_consts.rs` は条件付きOrderingを含む静的評価を確認する。`8aa6b02` の `tests/p2_static_strings.rs` は旧 `string_comparison::all_constant_comparisons_match_runtime_order_without_emitting_the_helper` の全5組×6演算をsource constへ移し、C戻り値とruntime helper非生成を確認した。

- code revision `af651d2` の `cargo test --workspace --locked --offline`: 188 pass、失敗/skip 0、終了0。fmt/Clippy全target・警告拒否も終了0。
- `8aa6b02` の `cargo test -p emela-emb-compiler --test p2_static_strings --locked --offline`: 1 pass、C両最適化pass。
- `tests/p2_standard_api.rs::code_point_capacity_defects_are_preserved_when_only_the_tag_or_no_value_is_used`: String容量2、heap4096で2byte scalarを1000回生成・解放し、3byte scalarはtagだけの利用と結果破棄の両方で `STRING_CAPACITY` / `string capacity exceeded` を観測した。旧 `dynamic_string_from_code_point_checks_capacity_before_writing` に対応し、C両最適化pass。

`tests/p2_debug.rs` の追加回帰は、todoの `TODO` kind/`not yet implemented` と動的panicの `PANIC` kind/整形messageをC hookで比較し、停止後のexternalを呼ばないことを両最適化で確認した。

generic trait methodのIR具体化は `ccd0615`、source収集・型検査への接続は `39f21d9` で検証した。`tests/p2_generic_methods.rs` はimplの型引数とmethod固有の型引数の順序、defaultからのgeneric呼出し、methodのbound、generic errorのfails/escapeをsource/Cへ通す。`4689729` は戻り値だけに現れるSelfを後続の利用から推論してからtraitを解決し、最後まで決まらないSelfは拒否する。constructorの入れ子の期待型は `a34aaa0` / `78be1b6` でsolverへ渡す。

`46897293793f6002a9c4c7aa0707352499706a3c` の統合確認:

- `cargo test --workspace --locked --offline`: 196 pass、失敗/skip 0、終了0。全C試験は`-O0`/`-O2`とも成功。
- `cargo fmt --all --check`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、実Rust 1.85.0での`cargo check --workspace --all-targets --locked --offline`: すべて終了0。
- 独立環境では当該revisionをgit archiveし、lock済みの第三者依存だけをvendor化した一時snapshotを使用。Linux Landlockで元workspaceの読取りを禁止し、clean CARGO_HOMEでworkspace build/test、CLI生成、生成Cの両最適化実行が成功。metadataの全packageはsnapshot内にあり、Emela packageはこのrepoの2 crateだけである。元workspaceへのアクセス拒否も実際に確認した。

最後の旧正常入力照合で発見した、fails省略時のgeneric error payload推論と具体型ごとのC表現は、以下の最終統合で修正・検証した。

旧`release_0_24/traits.emel`のList/closure/Fiber/Causeを含む元fixture全体の実行はP6以降に残す。同期部分の成功だけで旧混在fixture・旧115関連試験・横断要件全体をverifiedにしない。

## P2最終統合の結果

検証したsource revisionは `88eb3da2ed78282ab669e85ce46acf64b6d7a977`。WSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、`cc`（GCC 11.4.0）。MSRVは通常toolchain設定を変更せず、実際のRust 1.85.0のcargo/rustc/rustdocを指定した。

`ba11308` はerror payloadの制約を値型solverへ共有し、相互再帰群の汎化前に解決する。error集合だけに現れる型変数も汎化し、呼出しごとのInt/String等の具体化へ渡す。`tests/p2_error_inference.rs` の3件で直接呼出し・SCC内のescape・If/trait経由・Bool patternの共同網羅、Option[Int]/Option[String]の別instance、所有Stringの1000回再利用を確認した。payload不一致・pureへの失敗漏れ・guard付き部分回復を拒否する。固定spec 7.3と旧 `emela-types::body::infer_escape` に合わせ、escape最外のwildcard/変数/constも拒否する。

`643da4d` はIsError/AsErrorへ具体error型を保持し、同じgeneric宣言のInt版・Bool版等を別のAny payload slotへ正規化する。`88eb3da` はさらにOuterのOption fieldからNest・Innerを通るgenericな入れ子と未構築payloadの型を確認する。`tests/p2_failures.rs` の6件は明示fails・省略failsの両経路でC両最適化を通り、動的Stringを持つpayloadの寿命と1000回の全heap解放を確認した。既存のAny/tag/所有権rendererを維持し、P4以降の機能をP2へ混入させていない。

- `cargo test --workspace --locked --offline`: **201 pass、失敗/skip 0、終了0**。全C実行試験は `cc -std=c11 -Wall -Werror -O0` / `-O2` とも成功。
- `cargo fmt --all --check`: 終了0。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。
- 実Rust 1.85.0の `cargo check --workspace --all-targets --locked --offline`: 終了0。
- 上記revisionのgit archiveとlock済み第三者依存だけのsnapshotで、元Emela workspaceの読取りをLinux Landlockにより禁止。アクセス拒否を確認し、clean CARGO_HOMEで `cargo build --workspace --locked --offline`、全201試験、CLIのminimal source生成、生成Cの `-O0` / `-O2` 実行がすべて終了0。metadataはrepoの2 crateと通常第三者依存だけを含む。

P2の同期範囲を完了と判定する。直接依存はla-arena・serde・serde_jsonだけで、本体依存や新crateを追加していない。P3のList/Bytes/再帰値、P4の関数値/effect、P5以降のsuspend/取消、汎化const等の残件は計画と台帳で継続する。旧115関連試験と全source corpusの最終状態はP7まで個別に照合する。
