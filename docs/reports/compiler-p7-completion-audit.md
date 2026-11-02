# P7 終了条件の最終監査

[監査JSON](../compiler-p7-completion-audit.json) は、正本台帳commit `27fa16abe8fb3daa9e9135e0ce4a2bf28229632a` の状態・本文hash・各項目の証拠参照を保存する。最終compiler sourceは `b0a367bc0a29227fae62cee2def75ed925a5218f` で、本体workspaceを読めない独立snapshotの607試験・CLI生成・C両最適化が成功した。[検証報告](compiler-p7.md) と [実行記録](p7-final-validation.json) を参照。

| 集合 | 最終状態 | 未完了 |
| --- | --- | ---: |
| 428 records | verified 421 / not_applicable 7 | 0 |
| 226 contexts | verified 221 / not_applicable 5 | 0 |
| 115 tests | verified 82 / replaced 32 / not_applicable 1 | 0 |
| 10 requirements | 全件verified | 0 |

固定資料とのID/role/relation、context組、test名/path、requirement IDは完全一致。phase未定・source_scope未分類・欠落・追加・重複は0。867参照のpathが存在し、指定symbolの文字成分の欠落も0。以前の `compile_flow` 参照は独立Program/compile_programへの意味対応に修正した。

対象外は実在しないStage selectorとModule.stage、およびselector指定にしか意味を持たない旧試験に限定する。保持するHeap残余は拒否試験へ、source builtin8種はsource変換と残余拒否へ対応する。OnFail::Handleの旧拒否は、独立入力の検査・正規化と正常/失敗実行へ置換する。詳細は [schema対応](compiler-p7-schema.md) と [既知差分](../compiler-known-differences.md) に記録した。

115関連試験は [旧IR86](p7-ir-legacy-test-proposal.json) と [旧source29](p7-source29-legacy-test-proposal.json) に完全分割した。source29は原本文20件と既存試験の全観測包含9件を区別し、元の小容量も確認した。追加の [source対応表](../compiler-legacy-runtime-crosswalk.json) は正常174と拒否/構文3の177件を保持し、未照合は0。これらの集合を重複して加算しない。

共通99文脈、構造122文脈、Stage5文脈を個別に照合した。同期helperが中断rootから呼ばれたことを、命令自体の中断文脈の証拠にはしない。Literal Switch・dynamic Defect・Unreachable等は直接中断IRとC観測を追加し、不足を閉じた。

各proposalは査読時点の記録として保持する。最終状態は正本台帳と実行報告にあり、監査tool自身が意味論を証明したり自動昇格したりするものではない。再生成は公式ブランチの履歴とrepo内の固定資料だけを読み、旧checkout・未公開のエージェントcommitを必要としない。

```sh
python3 docs/tools/audit_legacy_completion.py --revision 27fa16abe8fb3daa9e9135e0ce4a2bf28229632a --check
python3 docs/tools/audit_source_runtime.py --check
cargo test -p emela-emb-compiler --test coverage_ledger --locked --offline
```
