# 旧fork対応表を使うコンパイラー移行の終了条件

[旧Flow IR→実装・試験対応表](reference/legacy-flow/flow-backend-inventory.md) / [定義一覧](reference/legacy-flow/flow-ir-inventory.md) / [文脈別の対応表](reference/legacy-flow/flow-coverage.md) / [実装候補](reference/legacy-flow/flow-implementation-inventory.md)

新repoで更新する正本は [移行台帳](compiler-coverage.json)。旧資料の出自・hash・加工内容は [provenance.json](reference/legacy-flow/provenance.json) に記録する。基準は旧forkの `c34c0aeefdd966d854371b3110db00a47798751d` であり、旧資料を新実装の進捗に書き換えない。

## 1. 現在の状態と目標

2026-11-03、P1〜P7を完了した。428定義・226文脈・115関連試験は根拠付き最終状態、追加10要件は全件verified。最終sourceの独立環境で607試験が成功した。判定の内訳・対象外の理由・再現条件は [P7検証報告](reports/compiler-p7.md) と [移行台帳](compiler-coverage.json) に記録する。

取り込み時点では、台帳の全項目が `pending` である。最小C生成は実装済みだが、この基準での対応付けと検証は未評価なので、自動で完了へ移していない。旧表の実装入口・関連試験・`candidate_found` を新repoの検証済み状態へ継承しない。

目標は、基準旧forkのコンパイラー相当のソース処理・C生成・runtime・公開境界を、このrepoだけで提供すること。物理BSP・startup・flash・実機確認は別の作業である。compilerが生成するC ABI、必要なサービス・hook・ISR情報と、hostのC mockによる実行は今回に含む。

本体のcrate・CLI・checkout・core/std bundleを、通常依存にもdev-dependencyにも追加しない。旧実装との比較は移行時の補助に限定し、新repoの必須試験を本体から独立させる。

## 2. 台帳の集合と集計単位

台帳の `schema_version` は1。`baseline_revision` と `baseline_provenance` が上記の固定資料を指すことを確認する。次の集合は件数だけでなく、キーの一致、重複・欠落・余分な行がないことまで確認する。

| 台帳 | キーと比較元 | 固定した範囲 |
| --- | --- | --- |
| `records` | `id`。旧backend表の全`records`と定義一覧の全`items` | 428行。195 variant、43共有宣言、190 field |
| `contexts` | `(id, context)`。旧coverageの`items[].contexts[]` | 226行。common 85、sync 58、suspend 58、input 25 |
| `tests` | `(target, name)`。旧backend表の関連試験を重複除去 | 115件。`legacy_path`も旧資料と照合する |
| `requirements` | `id`。第6節の10項目 | 10件すべてを維持する |

`records.relation` と `legacy_role` は旧表の値を保存する。宣言・fieldの233行を195 variantに加えて「対応機能数」と数えない。195 variantは一次監査168と追加metadata 27からなる。226文脈行は一次監査168項目についての206候補・20拒否根拠であり、195 variantの全情報を置き換えるものではない。

115件は旧表に登録された異なる関連試験であり、旧compilerの全試験数ではない。残りのsource corpusと横断試験は `requirements.source_corpus` で追う。集計では各集合の判定状態を別々に示し、合算した割合だけで終了を判定しない。

## 3. 段階と担当範囲

| 段階 | 完成させる機能群 |
| --- | --- |
| P1 | 独立backend採用、module・関数・名前解決・基本型、scalar・制御、C実行 |
| P2 | データ型、型推論・汎化・trait、失敗、String・数値変換、基本const |
| P3 | 固定heap、List・Bytes・再帰値・Any、所有権 |
| P4 | closure・高階関数、effect・handler・layer、共有状態 |
| P5 | suspend・継続・frame、非同期task、型付き完了通知 |
| P6 | Scope・Fiber・Deferred・Cause、取消・cleanup |
| P7 | 汎化const・型導出、C ABI・ISR・標準bridge、全件照合 |

`records.phase` は `null` または `P1`〜`P7`。`null` は担当段階の未判定であり、最終終了時には残さない。複数段階にまたがる項目は、完成を判定する最後の段階を指定し、途中の対応を `reason` と証拠へ記録する。`contexts` と `tests` にphase欄を追加せず、関連するrecordと証拠から追う。`requirements.phases` は横断する段階を示す。

`records.source_scope` は次を使う。

| 値 | 意味 |
| --- | --- |
| `unclassified` | sourceとの関係が未判定。終了時には残さない |
| `source` | sourceから処理・実行できることを確認する |
| `ir_only` | 旧forkでもIR入力で確認する範囲。source対応と数えない |
| `input_only` | 不正入力や入力契約の検証。言語機能の実行とは区別する |
| `schema` | 型宣言・field等の表現・意味・制約を確認する |
| `not_applicable` | 新しい入力契約に存在しないことを根拠付きで確認する |

## 4. 状態と完了の証拠

`records` と `contexts` には、次の状態を使う。全体終了時に許すのは `verified` と、条件を満たした `not_applicable` だけである。

| 状態 | 判定 |
| --- | --- |
| `pending` | 未判定・未評価。旧資料に実装があるだけでは進めない |
| `mapped` | 新実装・試験への対応先を特定した。実行・レビューが終わっていないため未完了 |
| `verified` | 新実装、必要な試験、実行結果、意味の対応を確認した |
| `not_applicable` | 旧入力契約が新repoに存在しないことを確認し、失われる意味がないことを説明した |

`verified` のrecordには `phase`、確定した `source_scope`、`new_implementation`、`evidence`、確認内容を説明する `reason` を揃える。source対象は独自frontendから生成C実行まで、IR限定対象は独自IRからC実行までを示す。入力検証は拒否試験、schemaは新表現・fieldの意味と関連試験を示す。親項目と証拠を共用してもよいが、そのfieldに適用できる理由を記す。

contextの `verified` は、その `sync`・`suspend`・`common`・`input` 経路に適用できる証拠を持つこと。同期の成功を中断経路の成功として扱わない。旧表に経路がない追加metadataも、record単位で確認を完了する。

`not_applicable` は、旧固有のstageや入力表現を廃止した場合に限る。`reason` と設計・実装レビューを示す `evidence` を必須とし、必要な意味をどの新表現・処理が担うかを記録する。「未実装」「試験が難しい」「現状使わない」は理由にならない。対応するcontext・旧試験も個別に判定し、recordの対象外指定から自動伝播させない。

### 旧rejectを誤って対象外にしない

旧表のreject 24項目のうち、次の8項目はsourceからlowerされるべき操作である。これらのrecordとcontextに `not_applicable` を使わず、sourceからの変換とCでの観測を含む `verified` を求める。

```text
emela_core::Builtin::{AssertFail, Dbg, DebugPrint, Inspect,
                     Panic, ShowNested, ShowString, Todo}
```

旧Flowに未変換の形で残ることを拒否しているのであって、assert・表示・defect等のsource機能を未対応にする規則ではない。残る旧Stage・Heap IR要素も、入力表現の不要性と、closure・所有権・失敗等の意味の必要性を区別する。

これら8項目のinput文脈では、独自lowerによる変換と、未変換の不正な入力が後段へ届かないことを確認する。旧Flowを受け取るAPIの再導入は要求しない。

### 旧試験の状態

`tests` も `pending`・`mapped` は未完了。最終状態には次を使う。

- `verified`: 旧試験の意味を維持する新試験があり、実行結果を確認した。
- `replaced`: 分割・統合・別の試験形式へ置き換えた。旧試験の全観点がどの新試験へ移ったかを `reason` に記し、`new_tests` と実行証拠を揃える。
- `not_applicable`: 旧専用入力契約にしか意味がない試験。理由と、代替する独自IR検証・構成検査・レビュー等を `new_tests` / `evidence` に記録する。正常sourceや必要な意味を除外するために使わない。

最終状態では根拠・対応または代替記録を必須にする。試験名だけの登録、旧試験結果、skipは実行証拠にならない。元fixtureが複数段階の機能を含む場合は、部分抽出との対応と、元fixture全体を確認した結果の両方を残す。

## 5. 証拠の形式と実行報告

`new_implementation`・`new_tests`・`evidence` の各要素は、`kind` と `path` を必須、`symbol` を任意とするobjectで統一する。例は形式の説明であり、現在の検証結果ではない。

```json
{"kind": "implementation", "path": "crates/emela-emb-compiler/src/backend/lower.rs", "symbol": "lower"}
```

`path` はrepo rootからの正規化した相対pathとし、対象ファイルをこのrepoに保存する。絶対path、repo外への参照、URL、上流checkoutを新実装の証拠にしない。`symbol` を使う場合は関数・試験・文書節等を実在する名前で特定する。`kind` はimplementation、source_test、ir_test、diagnostic_test、run_report、review等、証拠の役割が分かる値とする。

実行報告には少なくとも次を記録する。

- 検証した新repoのsource revision。未コミット差分を含めた場合は、再現できる差分と内容のhash。
- OS/target、RustとC compilerの実体・version、必要な設定・容量・入力fixture。
- 実行したcommand、結果・終了コード・期待値との照合、対象試験と失敗・skipの有無。
- 生成Cを `-O0` と `-O2` でコンパイル・リンク・実行した各結果。適用しない入力拒否試験等はその理由。
- 対象範囲に失敗・skip・未実行が残っていないこと。本体なしで成立する試験環境と、使用したC mock。

報告と実装・試験のrevisionが変わった場合は影響を確認し、関係する証拠を更新する。構造上の照合やレビュー記録を実行報告の代わりにしない。本書は手動レビューで適用する終了条件であり、自動完了判定・意味論の自動証明を提供しない。

`cargo test -p emela-emb-compiler --test coverage_ledger --locked --offline` は、固定資料とのID集合・関連情報の一致、重複、状態値、repo内の証拠参照、最終状態の必須情報を検査する。必要なsource操作を対象外にする誤りも検出する。試験内容とその実行結果が各項目の意味を満たすかは、引き続き個別にレビューする。

## 6. 表だけでは判定できない10要件

`requirements` は全10件を維持し、終了時はすべて `verified` にする。`not_applicable`・`replaced` や項目削除で省略しない。途中は `pending`・`mapped` とし、各要件に実装・試験・実行報告・レビューの `evidence` を紐付ける。

| ID | `verified` に必要な確認 |
| --- | --- |
| `source_corpus` | 旧115件以外も含む全対象source corpus・横断試験を一覧化し、移行先と実行結果を確認。旧正常sourceの退行を残さない |
| `frontend_validation` | 字句・構文・名前解決・型推論・trait・error/effect・capabilityの正常入力と拒否入力を検証 |
| `standard_library` | 旧compiler試験が必要とするcore/std/Embedded APIとintrinsicを独立供給し、互換動作を確認 |
| `memory_ownership` | 固定容量、borrow/own、深い解放、cycle、戻り値・capture・失敗・取消・未使用結果の寿命を確認 |
| `const_identity` | 汎化constの具体化をまたぐ単一初期化・共有・defect cache・managed rootを確認 |
| `concurrency_cleanup` | suspend/resume、task完了・stale/重複通知、取消、suspendするcleanup、contextの一度だけの解放を確認 |
| `c_abi_isr_services` | C ABI 2・schema 2・export・所有値境界・ISR到達性・外部保証・必要サービス・標準bridgeをC mockで確認 |
| `numeric_unicode` | Int/Int64境界、Float parse/format/丸め、UTF-8・書記素・Unicode vectorと容量制約を確認 |
| `standalone_execution` | 本体の通常/dev依存・CLI・checkout参照なしでビルド・生成・必須試験が成立し、対象C実行が-O0/-O2で成功 |
| `known_differences` | 旧source制限・旧IR限定機能・新旧差分・対象外を具体的に記録し、旧正常入力の新たな退行がないことを確認 |

## 7. 更新手順と最終判定

1. 固定資料から対象ID・文脈・関連試験を選び、recordのphaseとsource_scopeを決める。
2. 新実装・試験への対応をobjectで記録し、未検証なら `mapped` にする。
3. 必要なsource/IR/C実行・拒否試験を行い、repo内の報告と証拠を登録する。
4. 意味・所有権・資源・ABIの対応をレビューし、該当record・context・旧試験を個別に最終状態へ更新する。
5. 関連する横断要件を更新する。固定資料のhashと台帳の集合一致・参照先を確認する。

最終終了には、428 records・226 contexts・115 testsがすべて根拠付き最終状態であり、phase未判定・source_scope未判定・欠落・重複がなく、10 requirementsがすべて `verified` であることを求める。さらに証拠の適用revisionと、対象試験に失敗・skip・未確認がないことを人が確認する。

満たした時点で「固定した旧fork相当の独立コンパイラーへの移行完了」と判定する。基準の変更は別の差分として扱い、旧資料や台帳の行を黙って削って達成率を上げない。
