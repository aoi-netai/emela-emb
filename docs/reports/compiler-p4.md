# P4関数値・effect・共有状態の検証報告

P4は関数値・closure、高階関数、同期effect/handler/layer、State/Consoleを独自frontendからC実行へ接続する。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。suspend、Scope/Fiber/Deferred、汎化const等は後続段階として残し、同期の成功からそれらの完了を推定しない。

## ソースから接続した機能

| 範囲 | 確認した動作 | 主な試験 |
| --- | --- | --- |
| 関数値 | FnRef/Lambda/Apply、constructor pattern引数、関数・constructor・trait・intrinsic参照、遅延trait制約、Never戻り型とfailure/evidence ABIの拡大 | `p4_functions.rs`、`p4_function_values.rs` |
| 集合推論 | hidden/opaque error/effect row、包含と汎化、具体instanceの区別、関数型の変性、Immediate、型注釈のない高階lambdaと返却lambdaの潜在集合 | `p4_rows.rs`、`p4_effects.rs`、`frontend/{sets,infer,effects}.rs` |
| capture | 評価順、返却closure、借用String入力のsnapshot、nested List/tupleと失敗payload、呼出し元終了後の所有権 | `p4_closure_lifetimes.rs` |
| handler | effect/use、handler/init/release、layer依存順、上書き引数、label/default、lexical capability、静的・動的evidence、失敗時の逆順解放 | `p4_handlers.rs` |
| capability | 直接の返却・引数・data格納・値型注釈を拒否。withの寿命を越える保持closureを拒否し、その場で実行されるcallbackは許可 | `p4_capabilities.rs`、`p4_module_effects.rs` |
| State | 引数をsource順に一度評価、入れ子の異なるState、返却closureによる共有更新、String/Bytesのsnapshot・更新前の値・失敗・Cへの所有権移動 | `p4_state.rs` |
| 標準API | Option/List/Map/Setのcallback bodyを固定版へ復元。順序・短絡・安定sort・失敗途中の解放、ConsoleのState記録と明示C adapter | `p4_standard_callbacks.rs`、`p4_console.rs` |

handlerのoperationは構築時のevidenceを保持し、通常の関数値は呼出し時のevidenceを受け取る。withは取得に成功したhandlerだけを登録し、退役後にreleaseする。失敗を外側のescape armへ渡す前に必要なreleaseを完了する。未使用layerの不正な依存関係や引数も検査する。

`@export` と起動entryは未供給のeffectをCへ持ち出せない。失敗集合の公開ABIは維持する。

State.runはcompiler所有の宣言identityを基に、独自の具体化処理でCell、operation closure、Handler、Provideとcallback呼出しへ展開する。Stateはretireせず、返却closureが共有状態を保持できる。公開Cell型やStd.Cell APIは追加しない。

## 独自IRとCの境界

`backend/lower/function_tests.rs` と `handler_tests.rs` は、sourceから構築しにくい不正IRとABI境界を別に検証する。labelのevidence keyを区別したまま同じruntime handler ABIへ正規化し、関数型射影後の同一型を複合値まで統合する。失効したhandlerをAny alias経由で呼び出す場合もactive検査を維持する。

`backend/lower/cell_tests.rs` は固定旧 `tests/cell/mod.rs` のUnit共有identity、結果破棄のCellNew/Get、Cellを含む失敗payload、外部rootが残るCell/Any循環を独自IRで再構成する。追加retain/releaseの途中では値を保持し、最後のrootを解放した後に循環を回収する。Cell/closure/Anyの循環はhandler試験でも確認する。

所有権を検査する正常経路は原則1000回反復し、各回の全heap blockがfreeとなることを観測する。P4のBytes試験は、旧 `tests/bytes/mod.rs::bytes_captured_in_closures_and_state_keep_their_owners` の公開Cでの保持・元buffer上書き・元Bytes解放後の生存も移行した。旧500回を1000回に増やす。C compilerがない場合は試験を失敗とし、skipしない。

## 実装中に修正した回帰

- 再帰する高階関数で集合型を入れ子にし続けていた具体化を正規化し、List.fold等の同じinstanceを再利用する。
- inferred rowの既知の最小集合を参照し、内側State.runから伝わる要求を外側の明示providerで除去する。lambda引数・captureの潜在集合は早期にemptyへ固定しない。
- 推論に失敗した共有制約状態で後続の宣言を検査せず、無関係な標準moduleへの診断の波及を防ぐ。
- handler構築の判定でgeneric型へゼロ引数instanceを作らず、P2のgeneric errorを保持する。
- withの失敗配送に境界を設け、releaseより先にescape armが実行される経路を修正する。

## 最終検証

source revision: `5dab56cf63e90f299ca0f3b76023b68bdc5e71b1`。WSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、`cc`（GCC 11.4.0）。

- `cargo test --workspace --locked --offline`（独立snapshot）: 335 pass、失敗/ignore 0。全C実行は `cc -std=c11 -Wall -Werror -O0` / `-O2` とも成功。
- `cargo fmt --all --check` と `git diff --check`: 終了0。
- `cargo clippy --workspace --all-targets --locked --offline -- -D warnings`: 終了0、警告0。
- 実Rust 1.85.0による `cargo check --workspace --all-targets --locked --offline`: 終了0、警告0。
- Linux Landlockで元Emela workspaceの読取りを拒否した独立snapshotで、clean CARGO_HOME・lock済み第三者crateの一時vendorだけを使い、全build・全試験・CLI生成・C両最適化実行が成功。

最終版より前の `f8aab09` でも `cargo test --workspace --locked --offline --no-fail-fast` の333試験が成功した。最後の2件は未供給effectを持つC export・entryの拒否試験で、上記335件に含む。

独立snapshot: `/tmp/emela-emb-standalone-sytsi35m/repo`。ローカルログは `.local/experiment/p4-final-{tests,clippy,msrv,standalone}.log`。vendorと生成物は検証用の一時領域に置き、repoの依存には追加しない。workspaceはcompiler/CLIの2 crate、compilerの直接依存はla-arena/serde/serde_jsonのまま。

[移行台帳](../compiler-coverage.json) では同期source/IRの証拠を更新し、suspend文脈と後続段階の型・所有権条件は未完了として保持する。P5以降と旧全source corpusの監査を完了するまで、旧fork全体と同等とは表明しない。採用元と適応点は [出自](../migration-source.md) に記録する。
