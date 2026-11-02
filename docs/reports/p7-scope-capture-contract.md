# Scopeから返すclosureと固定旧の寿命検査

比較対象は固定revision `c34c0aeefdd966d854371b3110db00a47798751d`。戻り型に現れないFiber/Deferredのclosure captureは、固定旧と同じruntime検査の対象であり、source拒否の未実装として扱わない。

固定旧 `spec/language/11-concurrency.md` 11.8 は次の境界を区別する。

| 経路 | 固定旧の契約 | 独立compilerの確認先 |
| --- | --- | --- |
| Scope capability値の引数渡し・data格納・直接返却 | source拒否 | `frontend/capabilities.rs` |
| `Fiber.scope`の可視結果型にFiber/Deferredが含まれる | 集合確定後にsource拒否 | `frontend/concurrency.rs`、具体化時再検査、`tests/p6_concurrency.rs`負例 |
| 結果型に現れず、返却closureのcaptureやStateに保持されるhandle | 終了済みscopeを使用した時にruntime defect | 下記の元source/C観測。State格納経路の網羅はこの5試験だけから主張しない |

同節はcapture setsによる静的拒否を0.22への変更として説明する。固定旧 `crates/emela-types/src/body/capabilities.rs` の `check_scope_values` / `handle_in` も、名義型の引数を含む`Ty`の子を調べ、closure環境のprovenanceは追跡しない。独立checkerは同じ可視型境界を持つ。捕捉したhandleを持つclosureを全面的にsource拒否する変更は、次の固定旧正常入力と両立しない。

`p7_deferred_compat.rs`の `leaked_complete_callback_checks_scope_before_recycled_slot` と `ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_{poll,await}`、`p7_child_results_compat.rs`の `leaked_{join,await}_callback_*` は、本文を変えずC exportだけを付けた元入力を使用する。返却関数型にはhandleが現れず、環境内にだけFiber/Deferredを保持する。scope終了後に別のscopeを1000回作成・解放し、slot/frameを再利用してから古いclosureを呼ぶ。

5入力すべてでsource検査・C生成・C11 `-Wall -Werror -O0/-O2`のlinkと実行が成功し、それぞれ`fiber was joined/awaited after its scope ended`、`deferred was completed/polled/awaited after its scope ended`の専用defectを観測した。stale slotの偶発的な診断や値返却で代用しない。終端defectでは旧試験同様に停止hookがprocessを終了し、生存中の古いclosureを全解放したとの主張は行わない。

元入力hash、旧assert、実行設定と新試験の対応は `p7-concurrency-corpus-compat.json` に記録する。この比較に伴うcompiler挙動の変更はない。
