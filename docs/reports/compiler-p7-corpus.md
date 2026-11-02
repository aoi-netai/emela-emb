# P7 固定旧source corpusの監査索引

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` のtestを、source入力・IR・runtime・監査用fixtureへ分けた。[JSON索引](../compiler-legacy-source-corpus.json)は旧testの**期待**を記録する。新compilerでのsource/C成功やP7完了を示すものではない。既存coverage ledgerの状態は変更していない。

## 対象と数え方

依頼対象は `crates/emela-codegen-emb/tests` と `crates/emela-codegen-emb/src/compiler/tests` のRust 72files。Cargo既定のlib/integration targetから `mod`・`#[path]`・`include!` を辿り、参照先の `examples/support` とlibrary unit testsも含めて355 Rust filesを読んだ。対象72filesの未訪問と未解決module参照は0。

コメント・raw string・通常のstring・char・lifetimeを区別してからtest属性を抽出した。発見した488物理test functionsと、token化した488個の `#[test]` が一致する。`test_inventory.rs` のRust fixture文字列に含まれる `"#[test] fn ..."` はtestに数えない。以前の164files/651正規表現候補や、coverage ledgerの115個の関連testを全test集合として流用していない。

| 旧testの経路 | test数 | 意味 |
| --- | ---: | --- |
| source | 177 | Emela sourceを登録・解析・resolve・型検査・lowerする呼出しがある |
| IR-only | 266 | Core/Flow/lowered/emit contract等を直接構築する |
| runtime-only | 5 | C runtimeやUnicode実装等を直接検査する |
| infrastructure-contract | 40 | Rust依存境界、inventory、artifact/board package等を検査する |
| 合計 | 488 | 元から `#[ignore]` の2testも属性を保ったまま収録 |

source 177testのうち、正常期待174、resolve拒否期待2、同梱moduleのparseだけを検査する期待1。これはtest単位の経路分類であり、174個の新compiler成功を意味しない。

source呼出しへ結び付いた具体program本文はhashで共通化して167個。別に7個の生成式を未具体化のまま保持する。本文候補・文字列置換の断片・Rust fixture・module資産も含む全text記録は218個で、167個のprogram入力と区別する。1testが複数moduleを渡す場合は、`source_invocations[].inputs` にfilenameと本文IDの組を保持する。

## 期待と根拠

各testはtarget・正規module名・function名・固定Git blob・行を持つ。source本文はUTF-8 byte数とSHA-256、inline/include元を持つ。共通helperはtestからのcall traceを残す。

`support::try_lower` はparse/resolve/type diagnosticsが空であることを内部でassertしてからlowerの `Result` を返す。このため、その `.unwrap_err()` をfrontend拒否として分類しない。parse・resolve・typecheck・lowerの期待は別fieldにし、backendやC harnessの呼出しも別に記録した。単に `MemoryFiles::new` やparserを呼ぶだけの経路をC成功へ拡大していない。

[人手の経路review](../tools/legacy_corpus_reviews.json)は56testを補足する。Rust inventory用の `enum`/`fn` 文字列、native contract fixture、同梱sourceのparse loop、driverの2件のresolve拒否などが対象。reviewは旧testの意味だけを述べ、全recordの `new_status` は `unmapped` のままにしている。

## source変形とcross-target参照

相対 `include_str!` を固定tree内で解決し、module/include/asset参照をJSONに保存する。working treeの内容は読まない。

- Bytes corpusの `fn main()` → `pub fn main()`、`20000` → `128` を実際に適用した本文を保存する。include元の未変形本文を、そのtestが投入した最終sourceとして扱わない。
- aggregate ABIの2fileの `.concat()` は結合後のsourceと構成本文IDを保存する。
- `examples/support/phase7_integration_flow.rs` はsourceを書き換える条件分岐を持つ。元と変形後の候補を保持し、どの分岐が各testで実行されるかはreviewを残す。
- `sources.rs` の `bundled_sources()` は定数 `SOURCES` の6moduleへreviewで結び付ける。このtestの期待はparseのみで、typecheck/lower/Cを主張しない。

## 未完了の入力review

7個の `format!` 生成式と、2testで共有する条件付きsource書換えが残る。いずれも旧正常sourceの対象に残し、新compilerの不足syntaxを理由に除外していない。生成式はRust式そのもの、呼出し元のfunction本文、helper trace、sourceらしいliteral断片をJSONに残した。

| 生成/分岐元 | 残る作業 |
| --- | --- |
| `tests/completion_types/derived.rs` | type定義とtype名を差し込む生成sourceの組を具体化する |
| `tests/const_runtime/mod.rs` の2式 | loop/helperから渡すconst式の各variantを具体化する |
| `tests/deferred/mod.rs` | leakするcallbackの戻り型variantを具体化する |
| `tests/fiber/await_.rs` | child/scopeの生成source variantを具体化する |
| `tests/fiber/cause.rs` | cleanup/interruptionの生成source variantを具体化する |
| `tests/string_comparison/mod.rs` | 共通 `SOURCE` と追記sourceを具体化する |
| `tests/fiber/integration.rs` の2test | helperのbool引数とsource書換え分岐の対応を確定する |

この9test以外についても、索引は旧testを実行した証拠ではない。新compilerへ対応付ける際はsource受理、拒否、Cの各経路を個別に確認する。IR-onlyからsource成功へ格上げしない。

## 再生成と独立性

Python 3.10以降の標準libraryとGitだけを使う。旧repoでは固定objectに対する `git ls-tree` と `git show` だけを呼ぶ。旧compilerのbuild・実行、dirty checkoutの編集、本体crate/dev依存は追加していない。

```sh
python3 docs/tools/index_legacy_corpus.py /path/to/legacy/repository
python3 docs/tools/index_legacy_corpus.py /path/to/legacy/repository --check
```

生成時に、全test属性の対応・対象file/moduleの訪問・test ID重複・本文hash・source ID参照・reviewの固定revisionを検査する。`--check` は再生成結果とcommit済みJSONのbyte一致を検査する。

旧repoの指定はこの監査成果を再生成するときだけに必要。compilerや通常の `cargo test` は索引生成器を呼ばない。P7の実行試験へ使う本文はこの索引から必要な組だけ `crates/emela-emb-compiler/tests/compat` に保存し、C harness・期待値・出自を添える。最終試験が旧repoのpathやbundleを読む構成にはしない。
