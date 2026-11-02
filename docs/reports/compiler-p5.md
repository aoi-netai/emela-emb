# P5中断・再開と外部taskの検証報告

P5はsuspend・外部task・Clockを独立source frontendからC実行へ接続する。基点はP4の `72d7729`、固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。Scope/Fiber/Deferredと取消はP6、汎化constと残るC境界はP7で判定する。

## sourceから接続した機能

| 範囲 | 確認した動作 | 主な試験 |
| --- | --- | --- |
| 中断判定 | Asyncはevidence不要のruntime要求、通常effectはoperationと集合に従う。同期operationの実際の中断を拒否し、resume保持はhandler構築側へAsyncを要求 | `p5_suspend.rs` |
| 再開 | 即時・queue経由のone-shot resume、lambda・closure・関数参照、同期/中断/evidence付き関数ABIの適合 | `p5_suspend.rs`、`backend/lower/suspend_tests.rs` |
| task | 明示adapter、型付き成功/失敗、所有String・capture、producer/context lease、通知後のmain側実行 | `p5_task_failures.rs`、`backend/lower/suspend_tests.rs` |
| cleanup | 成功/失敗中のsuspendと元結果の保持、再帰frame、失敗payloadの外側escapeへの配送順 | `backend/lower/suspend_tests.rs` |
| 時刻 | Duration/Instant、SystemClock/TestClock、State[Instant]、nowだけを使うhelper、List.mapのeffect多相callbackからの順次再開 | `p5_time.rs`、`p5_clock.rs` |
| 診断 | written/derived Show、Showのないerror名、同じgeneric errorのInt/String payloadを具体型ごとに表示 | `p5_error_displays.rs` |
| CLI | task adapter、fiber/frame/task/event固定容量の明示指定、実CLIから生成したtaskの再開 | CLI `tests/build.rs` |

外部suspend関数の呼出し・参照はhandler operation内に限定する。未供給のAsync/effectはexportや起動entryへ持ち出せない。同期operationで中断するcalleeを呼ぶ場合も、source位置付きで拒否する。Immediateで具体化した通常wrapperから中断calleeへ橋渡しする経路は固定旧embedded plannerも対応していないため、独自runtime bridgeを新設しない。

producerと解放callbackは完了通知の場で実行せず、mainのschedulerから一度だけ呼ぶ。型付きString完了は借用inputの変更後も元の内容と埋込みNULを保ち、failureはsourceで構築したerrorからescapeへ到達する。success/failureを交互に1000回実行し、callbackがISR/lockの外で動くことと全heap回収を確認する。

## 独自IR・容量・所有権の境界

`backend/lower/suspend_tests.rs` はsourceでは構成しにくいframe・task・event不足、stale/重複通知、二重resume、lock復元を検査する。同期/中断関数値から中断evidence ABIへの拡大、外部task/内部frameへのFnRef、String producerの11byte保存先・同時保持・NULL出力・小容量defectを検証する。suspendを使わないImmediate handlerではschedulerを省略する。

String appendは入力と出力が重なる場合に両入力を固定容量snapshotへ退避する。最大512byteのscratchを用い、malloc・VLAを追加しない。`p5_string_alias.rs` は重複view、prepend/self-concat、0/1/512/513の容量境界、小caller bufferと10万回末尾再帰のstack非増加を確認する。Duration/Instantの負年・拡張年表記もこの修正を通る。出自は [migration-source.md](../migration-source.md) に記録する。

error表示関数は具体化後に登録し、runtimeからの暗黙呼出しを到達解析へ含める。静的constの不要なinitializer呼出しは除外する。CのString buffer・所有値の成功経路は両最適化で原則1000回反復する。

## 最終検証

source revision: `d7e8538a6f04e4ea4d526b37202e0a04af964c54`。WSL2 `x86_64-unknown-linux-gnu`、Rust 1.99.1、GCC 11.4.0。

- 独立snapshotの `cargo test --workspace --locked --offline`: **365 pass、失敗/ignore 0**。全Cは `cc -std=c11 -Wall -Werror -O0` / `-O2` とも成功。
- source境界の最終集中確認: Clock 3、suspend 6、task failure 1、error display 1の計11試験が成功。
- `cargo fmt --all --check`、`git diff --check`、strict Clippy全target: 終了0、警告0。
- 実Rust 1.85.0の `cargo check --workspace --all-targets --locked --offline`: 終了0、警告0。
- Linux Landlockで元workspaceの読取りを拒否し、clean CARGO_HOME・lock済み第三者crateの一時vendorだけで全build・全試験・CLI生成・C両最適化実行が成功。

独立snapshotは `/tmp/emela-emb-standalone-3g3pv6qo/repo`。ローカルログは `.local/experiment/p5-final-{source,clippy,msrv,standalone}.log`。workspaceはcompilerとCLIの2crateを維持し、Emela本体・Pome・旧checkoutへのbuild/test依存、新規第三者crateは追加していない。

中間版 `1a1210a` の358 passと、最終版の上記件数を区別する。対応台帳はP5で確認した項目だけを更新し、Scopeの集合・取消cleanup・旧corpus全照合の未完了項目はP6/P7へ残す。
