# P7 完成段階の検証報告

2026-11-03、固定旧fork `c34c0aeefdd966d854371b3110db00a47798751d` を基準とする独立コンパイラー移行の終了条件を満たした。P6確定 `a686172` を基点とし、最終source revisionは `b0a367bc0a29227fae62cee2def75ed925a5218f`。後続の変更は移行台帳・監査資料・説明文書のみで、compiler/CLI/試験のtreeは同一である。機械可読の結果は [最終検証記録](p7-final-validation.json) に保存する。

## 実装と修正

- 汎化constのcanonical initializer・型別bridge・依存具体化の固定点を実装した。具体化をまたぐ単一初期化、defect cache、外部opaque identity、所有値・GC rootとfailure/effect familyを維持する。
- record update、外部opaque型・generic externalの具体署名、external export wrapper、借用/所有/StringBuffer adapter、型付きtaskを接続した。
- Embeddedの6標準module/23操作とGPIO/UART/I2C/SPI/Clock/Consoleを供給する。ISR入口から到達するheap・動的call・再帰・中断・runtime const等を検査し、外部保証には実adapter symbolを使う。
- closure内Fiber失敗辺のID再配置、cleanupのevidence lookupのC label重複、frameの不要なpadding、最小Defect/Unreachable frameのstrict C warningを修正した。旧fixtureのheap/frame/pool容量を増やさず観測を維持する。
- 全体試験で検出した `dbg(call())` の中間値破棄を修正した。最後の新規Callだけを破棄対象にし、compound式の内部使用と、未使用childの即時回収を両立させた。[修正の根拠](compiler-p7-compound-discard.md) を参照。

## 固定した旧観測との照合

[移行台帳](../compiler-coverage.json) は固定ID集合を維持し、次の最終状態にした。variant、schema、文脈、試験を合算した割合では判定しない。

| 集合 | 最終状態 |
| --- | --- |
| 428定義 | 421 verified、7 not_applicable |
| 226文脈 | 221 verified、5 not_applicable |
| 115関連試験 | 82 verified、32 replaced、1 not_applicable |
| 横断10要件 | 全10 verified |

対象外は独立Programに存在しないStage selector・Module.stageと、その入力専用試験に限定した。保持するUnwind/MakeClosure/EnvGet/Dup/Drop等は入力拒否を確認し、source builtin8種はsource変換と残余拒否の両方を検証した。旧Flowでは拒否したHandleは、新frontendの合法入力として型と局所変数を検証しBlock/Breakへ正規化する。[既知差分](../compiler-known-differences.md) と [schema対応](compiler-p7-schema.md) に理由を残す。

[全source観測の対応表](../compiler-legacy-runtime-crosswalk.json) は、旧正常174試験・169原文programと、名前解決拒否2試験・標準source構文1試験を保持し、177件の未照合は0。旧355 Rust files/488試験の分類を保存し、115件だけを旧compiler全試験数とは扱わない。関連115件はsource29件と [IR86件](p7-ir-legacy-test-proposal.json) に分けて旧期待値へ対応付けた。

正常source174件の内訳は原本文runtime154、既存C観測による包含15、runtimeとartifactの混合1、runtimeとISR拒否の混合1、artifactのみ1、frontendのみ1、旧compile/headerのみ1。C生成だけでruntime成功とはしない。元本文hash、公開rootの追加、旧Options、C値・副作用・失敗・所有権の観測を各報告へ記録する。

旧試験にあるGC anchor、request外部化、Hold挿入、Protect cleanupの移動は独立IRで再現し、通常sourceだけの成功とは区別した。[Causeの2旧試験](p7-cause-ir-seam-compat.json) と [cleanupの12旧試験](p7-cleanup-ir-seam-compat.json) に後処理とC観測を記録する。通常の所有値経路は原則1000反復と回収を確認し、永続const rootは一定使用量、fatal defectは各C実行の1回停止として別記した。

## 最終検証

最終revisionのarchiveだけを `/tmp/emela-emb-standalone-cepa5r5t/repo` へ展開し、Landlockで元のEmela workspaceへの読取りを禁止した。空のCargo homeとCargo.lockにある第三者依存の一時vendorのみを使用し、workspace memberがcompiler/CLIの2crateであることも検査した。本体crate・CLI・checkout・core/std bundle・Pome・追加dev依存は必要ない。

| 検証 | 結果 |
| --- | --- |
| 独立snapshotのworkspace build/test | 607成功、失敗0、ignore/skip 0。docを含む80 test batch |
| 全対象生成CとCLI最小生成物 | C11のcompile/link/runが `-O0` / `-O2` とも成功 |
| strict Clippy、workspace全target | 成功、警告0 |
| 実Rust 1.85.0、workspace全target | 成功 |
| rustfmt、diff、最終台帳2試験・ID/参照照合 | 成功 |

環境はLinux 6.18.54.1-microsoft-standard-WSL2、x86_64、rustc/cargo 1.99.1、GCC 11.4.0。MSRVは実rustc 1.85.0で確認した。全体試験にはRust側の `CARGO_PROFILE_TEST_OPT_LEVEL=1` を指定したが、生成Cには常に `-std=c11 -Wall -Werror -O0` と `-O2` を使う（unit helperは `-Wextra` も指定）。C compiler不在・失敗をskipしない。

実行commandと結果は機械可読の最終記録に保存した。ローカルの独立実行logは `.local/experiment/p7-final-standalone.log`。文書・台帳更新後に台帳2試験と監査の再生成を別途実行した。source/IR対応表の実行参照は公式ブランチの祖先commitに揃え、サブエージェントの未公開commitを必要としない。最初の部分試験やP4/P6の受理調査を最終実行結果へ読み替えていない。

## 到達点

[定義した終了条件](../compiler-coverage.md) に従い、P1〜P7を完了と判定する。全Emela仕様や任意の入力の同値性を証明したものではなく、固定旧forkの定義・文脈・対象試験と公開契約に対する移行である。物理BSP、startup/vector/linker、firmware build/flash、実機、Pome取得、LSP・formatterは別作業とする。各段階は `gh stack` のPRでレビューでき、mergeは行わない。
