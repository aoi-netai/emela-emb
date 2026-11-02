# P4関数値・effect対応の検証報告（実装中）

P3の確定branchを基点に、関数値・closureとerror/effect集合の制約処理を接続中。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。ここでは独自IRの成功とsource対応を分け、P4全体の完了とは扱わない。

## 関数値・closureの基盤

`6f28495` の `src/backend/lower/function_tests.rs` は、返却された関数参照の到達性、Unitを含む間接呼出しのABI、返却されたclosureが保持するStringの所有化、loop変数のcapture時点の値を独自IRから確認する。String入力の元bufferを書き換えた後も結果が生存し、1000回の実行ごとにheapが全freeとなることをCで確認した。captureの欠落と循環したsequenceの不正IRも拒否する。

`frontend::specialize::tests::generic_function_references_inside_a_lambda_lower_to_indirect_calls` は型付きIRのFnRef/Lambda/Applyを具体化してCへ通す。generic関数参照とLambdaの型引数を共有したまま、backendが扱える具体的な関数型へ変換する。source checkerからこれらの式を作る接続は別の作業として継続する。

旧closure変換の採用元path・SHA-256と適応点は [出自](../migration-source.md) に記録した。const内のclosure、suspendを含む関数値はそれぞれ後続のconst・suspend対応と併せて確認する。

## 集合制約solverの基盤

`783d771` / `261d901` はASTや本体型システムへ依存しないerror/effect集合solverを追加する。tagの型引数を値型solverへ渡し、kindの区別、和集合・差集合・包含、上限制約と最小解、汎化・同時代入、Immediate制約を扱う。差集合は明示tagだけを取り除き、後で集合変数から現れるtagを遡及的に取り除かない。

11単体試験で、同一tagの型不一致、未確定集合の伝播、最初の集合パラメーターへの不足の割当て、未知のrigid/hidden集合をemptyやImmediateと誤認しないこと、上限制約の包含元への伝播、循環した包含の共有汎化、parameter位置だけの汎化、代入の順序を確認した。

現時点のmodule登録は `cfg(test)`。source型検査側のadapterと同時に通常pipelineへ有効化する。単体成功をsourceのeffect対応として数えない。

## 基盤統合の実行結果

source revision `261d901`。WSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、`cc`（GCC 11.4.0）。

- `cargo test --workspace --locked --offline`: 246 pass、失敗/skip 0、終了0。既存P1〜P3のsource/C回帰も含み、全C実行は `cc -std=c11 -Wall -Werror -O0` / `-O2` とも成功。
- `cargo fmt --all --check`: 終了0。
- 実Rust 1.85.0で `cargo check --workspace --all-targets --locked --offline`: 終了0。ただしsource checker未接続のExprKind variantにdead_code警告1件が残る。
- sets solver単独branchの `cargo clippy -p emela-emb-compiler --lib --tests --locked --offline -- -D warnings`: 終了0。P4統合全体のClippyは、前述の未接続variantが残るため合格扱いにしない。

P4の残件はsourceの関数型・FnRef/Lambda/Apply、error/effect集合adapter、effect/handler/layer、Cell/Stateとcycle、標準callback API。すべて接続してからsource/C・拒否入力・Clippy・独立環境を再確認し、[台帳](../compiler-coverage.json) に経路ごとの結果を記録する。
