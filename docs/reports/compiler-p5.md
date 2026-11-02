# P5 中断・再開と外部taskの検証報告（実装中）

P4の確定branch `72d7729` を基点に、suspend・task・Clockをsourceから利用できる段階へ進める。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。現在の独自IR検証と、source接続の完了を区別する。

## 統合した基盤

- 独自IRのsuspend関数・operation、frame変換、即時/遅延resume、同期関数値の中断ABIへの拡大を接続する。
- 外部taskの引数・型付き成功/失敗、producer/context leaseを扱う。完了通知はqueueへ積み、producerと解放callbackはmainのschedulerで一度だけ実行する。
- 中断中のclosure・所有String、再帰frame、cleanupの中断と元結果の配送順、stale/重複通知、task/event/frameの容量不足を検査する。
- error診断から暗黙に呼ばれる表示関数を到達解析に含める。静的constの不要なinitializer呼出しは除外したままにする。
- Duration/Instantの独立sourceを供給し、Clock宣言を準備する。Clockの登録とSystemClock/TestClockのsource実行を接続済み。

source revision `ac677e1` の `cargo test -p emela-emb-compiler --lib backend::lower --locked --offline` は60 pass、失敗/ignore 0。C実行は `cc -std=c11 -Wall -Werror -O0` / `-O2` の両方で成功。所有権・再開・cleanupの正常経路は1000回反復し、各回のheap回収を確認した。これは独自IR基盤の検証で、P5全体の完了を示さない。

String appendでは出力と入力が重なる場合も両入力を固定容量snapshotへ退避する。固定旧runtimeにあったalias破壊を修正し、Duration/Instantの負年・拡張年表記にも適用する。最大容量512 byteの一時領域を使い、VLA・heapを追加しない。出自と境界検証は [migration-source.md](../migration-source.md) に記録する。

CLIはtask adapterとfiber/frame/task/eventの固定容量を明示指定できる。Emela本体やboard packageへの依存は追加しない。実CLIからの生成とtask再開もC両最適化で1000回確認した。

## source接続と統合検証

`cc9387f` はPrelude.Asyncをruntime要求として扱い、普通のeffect evidenceと区別してFunction/Call/FnRef/Lambda/Applyの中断可能性を確定する。外部suspend関数の呼出し・参照はhandler operation内に限定する。custom effectからhost taskへ待機するlambdaと、直ちにresumeする同期exportをsource/Cで確認した。

`8247709` / `14923fe` はSystemClock・TestClockを明示task adapterへ接続する。State[Instant]の共有更新、負の待機時間のclamp、nowだけを呼ぶhelperの中断可能性、List.mapのeffect多相callbackによる順次再開を1000回・C両最適化・heap全回収で確認する。

`5c6c578` は固定旧の関数値ABI試験3件を追加する。同期/中断関数値から中断evidence ABIへの変換、外部task/内部frameへのFnRef、String producerの11byte保存先・同時保持・NULL出力・小容量defectを直接検証する。

source revision `1a1210a` で全workspace358試験が成功（失敗/ignore 0、全Cは両最適化）。strict Clippy全target・実Rust1.85全target check・fmt/diff checkも成功し、警告0。ローカルログは `.local/experiment/p5-integrated-{tests,clippy,msrv}.log`。後続のList.map回帰と関数値ABI3件は個別実行で確認済みで、この358件の件数には含めない。

## 残る完了条件

同期operation実装での中断制約、resume保持時のAsync要求、sourceからの型付きtask失敗と所有String、失敗表示関数の登録を補う。最後の点は、独自IRではerror_showsを明示供給できる一方、source経路からruntime診断へ渡す登録が不足している。

最終sourceで全体検証と元workspaceを読めない独立環境を確認し、対応表とこの報告を更新する。P6のScope/Fiber/Deferredと取消の完了は別に判定する。
