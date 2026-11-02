# P7 終了条件の残件監査

対象はP5確定docs `13cd3130094ac1f53ba99faa783df271a91b290f` の[正本台帳](../compiler-coverage.json)。source codeは `d7e8538`、独立snapshot全365test成功の結果を含む。今回の[監査JSON](../compiler-p7-completion-audit.json)は正本を変更せず、そのrevision・本文hash・各項目の状態・証拠参照・担当reviewを記録する。

## 集合と現在の判定

固定資料と428 recordのID/role/relation、226 contextの組、115 testのID/path、10 requirementのIDが一致する。欠落・追加・重複は0。正本にある実行済み状態も、未完了状態もそのまま保存した。

| 集合 | 正本の最終状態 | 未完了 | 補足 |
| --- | ---: | ---: | --- |
| records 428 | verified 115 | mapped 306 + pending 7 | phase未定228、source_scope未定53 |
| contexts 226 | verified 94 | mapped 29 + pending 103 | sync/suspend/common/inputを別に維持 |
| 関連tests 115 | replaced 18 | mapped 44 + pending 53 | 旧経路はIR-only 86 / source 29 |
| requirements 10 | verified 0 | mapped 8 + pending 2 | const identityとconcurrency cleanupがpending |

115件のうち、source29件は旧全source corpusの一部にすぎない。sourceの最終状態7件はBytesの所有権・容量・外部adapter・capture等の個別置換で、残り22件はsource/生成C/旧期待値の対応レビューが必要。全174正常source test・169programの保存とP4到達調査は別資料であり、未実行のC corpusは未完了のままである。

証拠参照435個は、基準または明示したcheckpointのGit blobに存在する。参照先pathの欠落0。`backend/mod.rs#compile_flow` は現行symbolに残らず、Moduleとその4fieldの5recordに旧名が残る。`compile_program` 等の現在の入口との意味を確認して正本参照を直す必要がある。symbolの文字成分があること自体は、意味の一致や実行成功を証明しない。

## 実装済みの経路と未完了の境界

- P1/P2/P3のscalar・制御・データ・String/List/Bytes・trait/errorにはsource/Cと独自IRの試験がある。未完了の旧IR行には、tupleのUnit位置、payload/nullary tag、generic field、invalid tag、literal switch、結果破棄のdefect等の**旧assertionとの照合**が残る。既存実装があることから一律verifiedへしない。
- この照合から見つけたinline data/分岐の9旧IR-only testは、`902175f` の独立IR 2testへ全C assertionを移した。[観測ごとの対応と実行結果](compiler-p7-ir-data.md)を保存し、旧IR限定のままC11/O0/O2/正常1000反復を確認した。正本の9statusはこの監査では変更せず、採用後の置換判定候補とする。
- P4のclosure・evidence・handler・StateとP5のsuspend/typed task/cleanupは独立snapshotの実行証拠を持つ。P5更新でFnRef/Closure/Call/evidence等はverifiedとなった。FnSets/Scopeを含む集合ABI、取消時cleanup、Fiber/Deferred/CauseはP6で完了させる。同期だけの成功をsuspend contextへ広げない。
- schemaの240行相当を機能数へ加えない。旧fieldの意味がどの独立表現・検査に移ったかを親子それぞれで確認し、phase未定228とsource_scope未定53を解消する。必要なsource機能を、旧入力形式の廃止と一緒に対象外へしない。
- 旧Stage・Heap専用入力の候補21record/15context/6testは、旧APIが存在しないという構造上の理由と、closure/所有権/失敗landing/取消等の意味の移行先を分けてreviewする。対象外判定は未確定で、JSONから正本へ自動伝播しない。

## 担当別の実作業

| 担当 | 残件 | 保存済み候補証拠・次の確認 |
| --- | --- | --- |
| frontend/backend P6 | ScopeのE/R包含、子provider、handle scope、Causeの空E、取消とsuspend cleanup | P5 task成功では完了しない。旧Fiber/Deferred/Cause sourceとC期待値を接続 |
| backend P7 | generic constのcanonical root fixed point・typed getter・source family・単一初期化/defect cache | baselineで拒否された正常8programとHandle[A]共有、unused const helperを再検証 |
| root P7 | external wrapper、StringBuffer/Borrowed/Owned失敗境界、ISR、Embedded services、8source builtin | 下記のcheckpointを統合し、旧観測と最終snapshotへ結ぶ |
| audit | 115旧testのassertion対応、schema/contextの未分類、全174正常sourceのC期待値 | 保存fixtureを使い、Options・entry/export・Flow後処理を明示して到達/C実行を更新 |
| root 最終検証 | 独立build・全試験・MSRV・strict Clippy・C両最適化 | P6/P7を含む最終revisionで実行。P5の365passを後続変更へ流用しない |

次のcheckpointは担当から成功報告を受け、Git内の試験宣言を確認した候補である。この監査では統合済み・最終実行済みの状態へ変えない。

| checkpoint | 試験/意味 |
| --- | --- |
| `2f2dc53` | Embedded 6module、Board 23symbol、source3test/C harness。旧Embedded正常source5件との対応が対象 |
| `c0f351d` | bare external opaque型、generic concrete external、constructor/pattern/field禁止、source3test |
| `0bc2668` | exported external wrapper正規化、StringBufferと同期error adapter、source4test |
| `ac756a7` | ISR source4test、raw Cell input1test、名前/alias/許可symbol・到達性 |
| `40f34c0` | 8source builtinのlowerとC観測、後段残余の拒否2test。8項目ともsource必須 |
| `70f2a13` | record update source3test。型引数変更・記載順→base・失敗cleanup・opaque・保持値/C両最適化 |
| `902175f` | 旧inline data/分岐9testの全C観測を独立IR 2testへ移植。source対応には加算しない |

基準compiler343個の物理testと、追加checkpoint22個のtest宣言をtoken化して索引にした。Rust fixture文字列内の `fn` / `#[test]` は数えない。これは試験発見の数で、CLIも含むP5全365testの実行報告や、各C harnessの成功数とは区別する。関数名一致は候補にしか使わず、発見だけで旧testとの意味の一致を確定しない。

## 再生成

Python標準libraryと、このrepoに保存されたGit object/旧corpus索引だけを読む。旧repoを参照せず、台帳を編集しない。

```sh
python3 docs/tools/audit_legacy_completion.py --revision 13cd313
python3 docs/tools/audit_legacy_completion.py --revision 13cd313 --check
```

reviewの入力は [legacy_completion_reviews.json](../tools/legacy_completion_reviews.json)。全未完了項目を担当reviewへ結ぶが、groupの共有はstatusの共有を意味しない。最終判定には[正本の終了条件](../compiler-coverage.md)に従った項目別レビューと実行証拠が引き続き必要である。
