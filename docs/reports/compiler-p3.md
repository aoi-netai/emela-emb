# P3固定heap・同期コレクションの検証報告

固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。source、独自IR、生成Cのどの経路を検証したかを分けて記録する。通常のビルド・必須試験は旧checkoutや本体crateを参照しない。採用した標準sourceのhashと適応内容は [出自](../migration-source.md)、最終状態は [移行台帳](../compiler-coverage.json) を参照する。

## 供給する同期機能

Listの構築・spread・空/非空pattern・型推論・網羅性検査を独自frontendへ接続した。Listの同期APIとEq/Ord/Showはcompiler所有sourceを通し、customな要素のtrait実装を呼ぶ。sortは安定merge sortを維持する。

Std.Bytesの宣言IDだけをnativeなBytes表現へ接続し、同名の利用者型とは区別する。所有buffer、slice・連結・List変換、UTF-8検証、hex、unsignedなEq/Ord/Showを提供する。Bytesの長さはStringの16bit長やString容量設定へ切り詰めない。

Stringのsplit/chars/code_points/concat/joinとASCII case変換をListへ接続した。Std.Map/Std.Setは固定旧版のweight-balanced tree、opaque境界、customなkeyのEq/Ord/Showを維持する。

callbackを受け取る公開APIはP4に残す。P3の同期API内部のcallbackだけを直接再帰helperへ適応し、P4で元のbodyへ戻せるよう出自を記録した。sourceに存在しない公開Any構文は追加せず、失敗集合の内部表現と独自IRのbox/unboxを区別する。

## sourceからCへの試験

すべて共通helperで `cc -std=c11 -Wall -Werror -O0` と `-O2` の生成Cをコンパイル・リンク・実行する。C compilerの不在、失敗、容量不足をskipにしない。

| 試験 | 確認した意味・境界 |
| --- | --- |
| `tests/p3_lists.rs`（5件） | 要素の型、spreadの評価順、Unit/Int64/String、空/非空とBoolの共同網羅、1000nodeの末尾再帰・100回の全解放、同期API、custom trait、stable sort、dbg fallback |
| `tests/p3_memory_api.rs`（2件） | String/List/Bytes標準API、UTF-8・hex不正入力、利用者の同名Bytes型と信頼済み宣言の分離、公開範囲 |
| `tests/p3_unicode.rs`（2件） | Unicode18公式全853 vectorの各clusterのbyte境界、全code point、join再構成を4KiB heapで3回実行。不正UTF-8は結果を使わないchars/code_pointsでも同じdefect |
| `tests/p3_ownership.rs`（3件） | List[(String,Option[String])]から取り出したStringが元List解放・stack上書き後も生存、借用入力のsnapshot、共有tail、tuple/enum・Batch失敗payloadの寿命、1000回の全heap解放、所有String部分viewの比較前後でheap使用量が不変 |
| `tests/p3_ownership.rs` の再帰値 | 型引数が変わる有限のNestを1000回構築・解放。Stringを含む深さ20000のChainを5回構築し、反復的な全解放を確認 |
| `tests/p3_bytes.rs`（6件） | 70000byteのheapなしview・size/get/slice、String容量から独立したBytes容量、alias/借用snapshot/所有adapterと全解放、0/128/255・不正byte、UTF-8/NUL/BOM/最大scalar・不正UTF-8、hex・unsigned比較、heap 0拒否・枯渇、70000byte decodeのString容量検査 |
| `tests/p3_collections.rs`（3件） | 昇順/降順120nodeの木、更新前の値を保つinsert/remove、keys/value順、重複key、left-biased union、custom key、Show、Set操作、opaque/bound拒否。正常な2入力を各1000回実行し、毎回heap全解放 |

所有権の試験は `tests/support/heap.rs` が試験用に生成Cへ監査関数を追加し、全heap blockがfreeに戻ることを観測する。製品の公開ABIへ監査機能を追加しない。malloc/callocの非生成も確認する。

Unicodeのclusterを返す関数は生成headerに従って所有者を持ち得るStringを受け取り、検査後にownerをreleaseする。joinのcaller buffer契約とは区別する。

## Map/Setの容量確認

初期の64KiB設定では、複数のpersistentな120nodeの木と比較用Listを同じscopeに保持する入力がheapを使い切った。位相ごとの使用量を調べ、次を両最適化で確認した。

| 時点 | 使用byte |
| --- | ---: |
| 昇順120nodeの木を保持 | 23040（120×192） |
| 降順の木も保持 | 46080 |
| keysと比較用rangeを同時保持する区間 | 69120 |
| 入力全体の最大保持量 | 85184 |
| 関数から戻った後 | 0 |

2回の観測が同じであり、自己末尾呼出しの古いaccumulatorや前回の呼出しの値を保持し続けていないことを確認した。必要な同時保持量に合わせて試験設定を128KiBへ変更し、`-O0`/`-O2`それぞれ1000回の正常結果と全heap解放を確認した。

## 独自IRと再帰表現

`src/backend/lower/memory_tests.rs` の9件は、List/Bytes/Anyの型検証、固定heapの10000回再利用、戻り値・tuple/List要素内の参照、深さ20000の解放、未使用Listの容量defectをCで確認する。Anyへのbox/unbox自体はIR限定の検証として記録する。

`b225f71` は旧 `any_boxes_retain_managed_list_payloads_until_the_box_is_released` のList[Int](17)→Any box→List unbox→head(17)を再構成した。固定4KiB heapで10000回実行し、毎回の全block解放も観測した。`cargo test -p emela-emb-compiler --lib any_boxes_retain_managed_lists_and_release_their_payloads --locked --offline` は1 pass、Cの両最適化とも成功。公開sourceのAny構文対応とは数えない。

増大型 `Growing[A]` の未使用payloadを無限に型展開しないよう、型パラメーターの伝播を調べて再帰辺を箱にする。相互再帰・型引数の入替え・定数型へのresetを区別し、公開Cからのaggregate戻り値のtagも確認した。型引数が増えることだけを理由に、正常な未使用payloadを拒否しない。

## 先行統合の実行結果

source revision `d43ee10`。WSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、GCC 11.4.0。

- `cargo test --workspace --locked --offline`: 220 pass、失敗/skip 0、終了0。全対象Cの両最適化を実行。
- `cargo fmt --all --check`: 終了0。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0。

標準Mapのconstを追加した際に、errors単体試験のhelperだけがconst展開を省略していたことを検出した。通常pipelineと同じ `collect → consts::expand → check` へ揃え、製品のconst処理を変えずに単体試験を修正した。

この結果はP3先行branchの確認である。P2の最終修正を正式stackへ統合した後の全体・MSRV・独立環境確認は、最終追記で記録する。Cell/closureのcycle、suspend・取消をまたぐ所有権、callback APIはP4以降に残し、同期成功からverifiedを自動で広げない。
