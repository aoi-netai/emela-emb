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

source呼出しへ結び付いた具体program/module本文はhashで共通化して179個。7個の生成式も12variantへ展開した。本文候補・文字列置換の断片・Rust fixture・module資産も含む全text記録は230個で、179個のprogram/module本文と区別する。1testが複数moduleを渡す場合は、`source_invocations[].inputs` にfilenameと本文IDの組を保持する。

## 期待と根拠

各testはtarget・正規module名・function名・固定Git blob・行を持つ。source本文はUTF-8 byte数とSHA-256、inline/include元を持つ。共通helperはtestからのcall traceを残す。

`support::try_lower` はparse/resolve/type diagnosticsが空であることを内部でassertしてからlowerの `Result` を返す。このため、その `.unwrap_err()` をfrontend拒否として分類しない。parse・resolve・typecheck・lowerの期待は別fieldにし、backendやC harnessの呼出しも別に記録した。単に `MemoryFiles::new` やparserを呼ぶだけの経路をC成功へ拡大していない。

[人手の経路review](../tools/legacy_corpus_reviews.json)は65testを補足する。Rust inventory用の `enum`/`fn` 文字列、native contract fixture、同梱sourceのparse loop、driverの2件のresolve拒否、format生成と条件分岐などが対象。reviewは旧testの意味だけを述べ、全recordの `new_status` は `unmapped` のままにしている。

## source変形とcross-target参照

相対 `include_str!` を固定tree内で解決し、module/include/asset参照をJSONに保存する。working treeの内容は読まない。

- Bytes corpusの `fn main()` → `pub fn main()`、`20000` → `128` を実際に適用した本文を保存する。include元の未変形本文を、そのtestが投入した最終sourceとして扱わない。
- aggregate ABIの2fileの `.concat()` は結合後のsourceと構成本文IDを保存する。
- `examples/support/phase7_integration_flow.rs` はsourceを書き換える条件分岐を持つ。`run(false)` と `run(true)` を呼ぶ各testに対し、対応するhelper traceと、Int/String成功型の最終sourceをreviewで確定する。`with_collector` によるIR差はsourceの差と分ける。
- `sources.rs` の `bundled_sources()` は定数 `SOURCES` の6moduleへreviewで結び付ける。このtestの期待はparseのみで、typecheck/lower/Cを主張しない。

## 生成式と条件分岐の具体化

7個の `format!` 生成式と、2testで共有する条件付きsource書換えをreviewで解決した。未具体化の生成式・入力reviewは0。生成式自体、呼出し元のRust function本文、helper trace、元のliteral断片もJSONに残した。新compilerの不足syntaxを理由に除外していない。

| 生成/分岐元 | 具体化 |
| --- | --- |
| `tests/completion_types/derived.rs` | 固定Wrap定義・`Wrap[Int]`・`Box([42])` の1variant |
| `tests/const_runtime/mod.rs` の2式 | doubleの17重適用1variant、panic/自己参照2variant |
| `tests/deferred/mod.rs` | poll/awaitと、対応する戻り型・invoke宣言の2variant |
| `tests/fiber/await_.rs` | join/awaitの2variant |
| `tests/fiber/cause.rs` | mode=0,1,2に対応するprotected関数の3variant |
| `tests/string_comparison/mod.rs` | 共通 `SOURCE` assetとstore/read追記の1variant |
| `tests/fiber/integration.rs` の2test | helperのbool引数に合わせたInt/String sourceを選択 |

sourceの具体化は、旧testを実行した証拠ではない。新compilerへ対応付ける際はsource受理、拒否、Cの各経路を個別に確認する。IR-onlyからsource成功へ格上げしない。

## 旧repoに依存しない正常source fixture

正常期待174testを、[fixture manifest](../../crates/emela-emb-compiler/tests/compat/legacy-corpus/manifest.json)から169program/170content filesへ共通化した。元のlogical filename・entry・test名・本文SHA-256を保持する。差し込んだ生成sourceを含め、source本文へC exportの注釈やwrapperを暗黙に足していない。

phase7 integrationのrequest外部関数化/GC anchor、Cause cleanupのFlow後処理などは `execution_configuration` に分ける。これらの旧C期待値を、後処理なしのsource実行と同一視しない。新compilerでの到達・C期待値の移植は、入力保存とは別の検証で記録する。

## P4最終版での到達調査

[到達JSON](../compiler-legacy-source-reach.json)は、新compiler `5dab56c` のrelease CLIで保存済み169programを調べた結果である。compiler binaryとfixture manifestのSHA-256を記録した。heap 131072、String容量256の共通設定を使い、旧test固有のOptions・外部adapter・Flow後処理は適応していない。後続のP5/P6/P7修正をこの基準結果へ混ぜていない。

| 入力/検証 | 結果 | 範囲 |
| --- | ---: | --- |
| 元sourceのcompile完了 | 49 / 169 program | 型検査とC生成まで完了。うち45programはC entry/export未選択 |
| 元sourceのbackend拒否 | 2 / 169 program | pipeline上で型検査は通過、C生成は未達 |
| その他のcompile拒否 | 118 / 169 program | このprobeだけでは型検査成功を確定しない |
| 具体関数を1個ずつexportしたC生成 | 81 / 95 root | 元sourceの49programにある具体main/public fnを対象 |
| 上記CをC11でobject化 | 80 / 95 root | `-std=c11 -Wall -Werror -O0/-O2` の双方 |
| 上記C生成拒否 | 14 / 95 root | 型検査後のbackend診断 |
| 上記Cのobject化拒否 | 1 / 95 root | 未使用const helperによるWerror |
| C link/実行 | 未検証 | object成功を旧C期待値の成功に数えない |

旧 `Roots::Public` の根拠は `crates/emela-core/src/lower.rs:3404-3421`。entry moduleのmain/public fnを選び、trait boundのあるschemeを除く。調査ではこのうち具体型の関数を1個ずつ明示exportへ対応付けた。明示generic headerとexternal宣言は具体ABI adapterのreview対象にし、generic export拒否を言語不足に数えない。型推論でgenericになった場合も具体exportの制限として分類する。今回49programの95対象はいずれも具体signatureで、個別exportによる型検査は全件通過した。private mainへのpub追加・C symbol・元位置を各recordに残し、元fixtureは変更していない。

raw compileの最初の診断による暫定振分けは、P6 source未接続86、P5 suspend/Async未接続19、Embedded service 5、P7要review10。これは全診断の完全な意味分類ではない。P7の10programはpolymorphic const関連8とbare external type関連2で、担当別の修正後に再調査する。具体root追加で現れたaggregate external ABIのadapter不足、const closureのC生成不足も到達JSONへ残した。旧正常sourceを不足syntaxのため対象外へ落としていない。

record updateはこの169programとは別に固定spec 5.3から確認したP7の不足で、`User(name: "b", ..user)` が構文段階で拒否された。既存AST/checkの拡張と、旧仕様の型引数変更・named RHS→baseの評価順・opaque境界の検証を別commitで行う。

## 再生成と独立性

Python 3.10以降の標準libraryとGitだけを使う。旧repoでは固定objectに対する `git ls-tree` と `git show` だけを呼ぶ。旧compilerのbuild・実行、dirty checkoutの編集、本体crate/dev依存は追加していない。

```sh
python3 docs/tools/index_legacy_corpus.py /path/to/legacy/repository
python3 docs/tools/index_legacy_corpus.py /path/to/legacy/repository --check
python3 docs/tools/materialize_legacy_fixtures.py
python3 docs/tools/materialize_legacy_fixtures.py --check
python3 docs/tools/check_legacy_reach.py --compiler-revision 5dab56c
```

生成時に、全test属性の対応・対象file/moduleの訪問・test ID重複・本文hash・source ID参照・reviewの固定revisionを検査する。`--check` は再生成結果とcommit済みJSONのbyte一致を検査する。

旧repoの指定は索引を固定objectから再抽出するときだけに必要。fixture生成器は保存済みJSONだけを読み、旧repoを参照しない。compilerや通常の `cargo test` は索引生成器を呼ばない。C harness・期待値・adapterは実行試験として個別に移す。最終試験が旧repoのpathやbundleを読む構成にはしない。
