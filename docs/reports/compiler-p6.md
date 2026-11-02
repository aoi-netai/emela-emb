# P6 構造化並行処理の検証報告

P6のsource接続・取消・所有権の段階検証を完了した。最終compiler revisionは `cb56194a1cb5e8c1ac5d1b4f1cf21d2428804dbf`、基点はP5 `13cd313`、固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。旧正常source・IRの全件照合はP7で継続する。

## 実装と観測

- Std.Fiber/Scope/Deferred/Causeを独立sourceへ接続した。callbackのfailure ABI、Unit/Any、phantom row、childごとのerror/effect包含、Scope地点のevidenceを保持する。
- Join/Attempt、Deferredの一度だけの完了・poll・FIFO再開、typed Causeの分類・表示・再送出を検証した。both/race/all/map、入れ子Scope、String/Listの所有値もCで実行する。
- 取消時は入れ子cleanupを逆順に実行し、中断するcleanupの完了後に結果を配送する。typed failureではreleaseを実行し、defectは即時停止する。同期defect後に到達しないcleanup helperは生成しない。
- ready状態のjoin/await配送、Deferred待機リンクのhead/middle/tail取消、FIFO interrupt、複数失敗noteを確認した。未使用の新しいCall戻り値を直ちに破棄し、完了childを親の次の待機前に回収する。既存localの読取りでlive handleを破棄しない。
- closure変換後に残る元のstatement arenaは、現在の関数bodyから到達する文だけを結果破棄の対象にする。外部opaque構文とmetadataは標準Fiberの前提として導入した。一般の外部ABI検証はP7へ残す。

旧原本文のlifecycle 4入力とchild waiter 4入力は、計10旧source試験の観測へ対応する。入力hash・旧名・C観測は [child waiter対応](p6-child-waiters-compat.json) と [採用記録](../migration-source.md) に記録した。公開C rootの追加と明示task adapterを使用し、child数・弱参照リンクはprivate C probeで確認する。overlapのみ旧試験と同じsource後のHold挿入を使い、純source経路と区別する。

## 最終検証

2026-11-02、上記revisionのarchiveを `/tmp/emela-emb-standalone-a1y90q4u/repo` へ展開した。Landlockで元のEmela workspaceを読めない状態にし、空のCargo homeとlock済み第三者crateの一時vendorだけで検証した。workspace memberはcompiler/CLIの2crateのみで、本体crate・checkout・corebundle・Pome・追加dev依存を要求しない。

| 検証 | 結果 |
| --- | --- |
| 独立snapshotのworkspace build/test、CLI生成 | 398 pass、失敗/ignore 0 |
| 生成Cのcompile/link/run | `cc -std=c11 -Wall -Werror -O0` / `-O2` とも成功 |
| strict Clippy、workspace全target | 成功、警告0 |
| 実Rust 1.85、workspace全target | 成功 |
| rustfmt、diff whitespace、台帳ID/証拠検査 | 成功 |

通常環境はRust 1.99.1・GCC 11.4・WSL2 x86_64。独立全体試験はRust側に `CARGO_PROFILE_TEST_OPT_LEVEL=1` を使用し、C側の両最適化条件は維持した。source基礎2試験は通常のRust test profileでも成功済み。所有値の正常経路は1000回反復し、旧容量設定・NULL出力・取消・stale通知・全heap回収を確認する。

ローカル実行ログは `.local/experiment/p6-final-{standalone,clippy,msrv}.log`。独立snapshotは全55 test group、398試験。最終文書更新後の台帳2試験は別に実行する。先行checkpoint `1086f6a` の386試験や受理だけのsource監査を最終結果へ混ぜない。

## 台帳とP7への引継ぎ

実在する22 test symbolとファイルhashを照合した更新候補に基づき、Fiber操作・型/集合契約・割込maskの46定義、25文脈をverifiedへ更新する。Protectには取消・中断cleanup・即時defect停止の根拠を追加する。428定義・226文脈・115関連試験・追加10条件のID集合を保持し、機能smokeの成功だけで旧115試験を完了へ上げない。

P7では正常source174試験と旧IR観測を全件照合する。待機5操作の同期caller拒否、closure内の失敗辺、複合値/const/外部ABI/ISR/標準bridge、cleanup/notesの全観測を引き続き検証する。Scope handleを返す可視型はsourceで拒否し、closure captureを通じた寿命超過は旧試験のruntime guard契約と区別して確認する。
