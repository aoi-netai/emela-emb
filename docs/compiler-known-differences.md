# 固定旧forkとの比較範囲と契約の違い

比較基準は `c34c0aeefdd966d854371b3110db00a47798751d` の組み込みコンパイラー。対象は [7段階の計画](compiler-parity-plan.md) と [終了条件](compiler-coverage.md) で固定する。旧repoの未コミット差分や、その後のEmela仕様は比較基準へ含めない。

## ソースの入口とC公開

`pub` はEmela内の可視性、`@export(c, "名前")` はC公開を指定する。旧試験の公開rootを新repoで実行するときは明示exportを加える。旧試験がprivateな `main` をCから呼んでいた場合は、その宣言に `pub` も加える。式・関数bodyを書き換えた代替試験と、公開属性だけを加えた原文試験は区別して記録する。

起動関数は `Options::with_entry` / CLIの `--entry-function` でも指定できる。起動entryには `pub` やexportを要求しない。ISRはさらに `Options::with_isr_entry` でEmela関数名を指定し、到達する外部呼出しには実際のadapter symbolへの保証を要求する。

moduleは呼出し元が渡すソース群と、このrepoに同梱するcore/std/Embeddedから解決する。compilerは本体のCLI、checkout、Pome、module loaderを呼ばない。C bindingのGlobal/Named/File/Bundled metadataは独立IRで保持・検査し、C symbolへ解決する。

## 維持する組み込みtargetの制約

| 境界 | 契約と確認先 |
| --- | --- |
| メモリ | 固定heap・各poolを使い、システムmallocへ逃がさない。容量不足はdefect。借用値のsnapshot、所有値のrelease、cycle回収は [対応仕様](language-subset.md) に従う |
| defect | board hookで即時停止し、cleanup unwindしない。通常の回復可能な失敗・取消ではcleanupを実行する。一般Emelaのdefect unwindとの違いを維持する |
| 実行文脈 | scheduler・producer・所有値の解放はmain側で実行する。ISRからの完了通知はqueueへ積む。任意のthread/coreから再入できる契約にはしない |
| Scopeの寿命 | 可視戻り型にScope/Fiber/Deferredを出す操作はsourceで拒否する。closureやStateに隠れたhandleは旧契約と同じruntime guardで検査する。[寿命境界の照合](reports/p7-scope-capture-contract.md) を参照 |
| C ABI | ABI 2・artifact schema 2を使い、生成headerを署名とlayoutの正本にする。内部symbol、番号、出力順やC本文のbyte一致は要求しない |
| 汎化・外部値 | 汎化関数を直接C公開しない。外部関数は具体化後のC署名が同一symbolで整合する必要がある。opaque external型のconstructor・field・patternは公開しない |

ASCIIの識別子、明示した関数引数型、対応する字句・構文、error/effect集合の制約等は [対応仕様](language-subset.md) と拒否試験で示す。固定旧版にないAPI（例: `Int64.parse`）を追加した扱いにはしない。全Emela仕様・全target・すべての正常入力に対する同値性の証明ではなく、固定した定義・文脈・旧試験の観測を根拠に移行を判定する。

## 独立IRの境界

旧Core/Flowを受け取るRust APIの互換性は設けない。旧 `Module.stage` と5個のStage値を指定するAPIを廃止し、独立入力・LoweredModule・PlannedModuleが変換の段階を所有する。旧stage宣言を含む7行のみ、意味の移管先を説明したうえで台帳の対象外候補にする。

既成のMakeClosure/EnvGet、env、Dup/Drop、saved/state、defect landing、Unwind、Heap専用runtime残余を受け入れない。これらは単なる対象外にはせず、入力拒否と、closure・所有権・中断・cleanupを生成する正規経路の試験を対応付ける。旧source builtin8種は独立frontendから変換し、後段に未変換の形が残る入力も拒否する。[schema対応](reports/compiler-p7-schema.md) と各input文脈の台帳を参照。

旧試験にsource lower後のHold挿入、Protect移動、request外部化、GC anchor追加等がある場合は、その後処理も独立IRで再現する。通常sourceだけで実行した試験として数えず、[source観測対応表](compiler-legacy-runtime-crosswalk.json) の `post_lowering_ir_seam` に明記する。生成Cのprivate cancellation flagやheap observerを使う試験も、公開ABIの利用例とは区別する。

到達しない具体化済み関数をarenaに残す場合があり、旧試験の総関数数と一致しない。再帰型・Showの試験では、C生成へ到達する関数数と必要なShowの具体化を確認し、保持された未使用metadataとの差を [再帰型の照合](reports/p7-recursive-source-compat.json) に記録する。

## 今回の範囲外

物理BSP、startup/vector/linker、firmware build/flash、実機、Pomeによる取得、LSP・formatter、platform接続製品は別作業とする。標準サービスのC hook・header・manifestとhost mockは今回のcompiler試験に含める。旧native platformのCコードは試験fixtureとして保存し、platform用Rust crateやビルド機構は追加しない。

最終統合revisionと試験結果は [P7検証報告](reports/compiler-p7.md)、行単位の判定は [移行台帳](compiler-coverage.json) を正本とする。この文書だけで未実行の行を完了にしない。
