# P7 旧Heap入力の残余検証

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の `tests/flow.rs`（emela-codegen-emb crate）にある次の入力観測を独立contractで確認した。

- `rejects_heap_only_runtime_operations_at_flow_stage`（195行）: Runtime Defect/DefectAt/DefectTake/DefectResume/Yield/YieldPerformの6入力は新contractにも残る。旧と同じ引数を使い、native/suspend双方で、operationの値検証がUnsupportedとなることを確認した。suspend機能未接続など別の入口拒否では成功にしない。
- `rejects_heap_only_unwind_and_explicit_landing_edges_at_flow_stage`（127行）のうち、Stmt::Letの明示on_defect/on_suspendは新contractにも残る。どちらもstatement検証でUnsupportedとなることを確認した。

残る旧Stage selector・Term::Unwind・Value::EnvGet/MakeClosure・Stmt::Dup/Drop・OnFail::Handleは、同名の旧入力表現を持たないものと、意味を新表現へ移したものを個別にreviewする。この2testから一律にnot_applicableへせず、sourceのclosure/所有権/失敗/中断cleanup等の意味を除外しない。旧Unwindを含むtest全体の判定も、上記landingの部分観測だけでは完了にしない。

移植先は `backend/lower/p7_input_tests.rs` の2test。正式P5 `13cd313` を取り込んだP7 worktreeで実行し、両方pass。意図した不正入力なのでC生成・実行は対象外であり、skipではない。strict Clippy全target、実Rust 1.85.0全target check、fmt/diff checkも終了0。旧repoへの通常依存・必須試験参照は追加していない。
