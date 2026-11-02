# P7 完成段階の検証報告（全件照合中）

P6確定版 `a686172` を基点に、汎化const・外部ABI・ISR・標準bridgeと旧forkの全観測を接続している。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。この段階はまだ未完了で、PRはDraftとする。

## 統合した機能

- 汎化constのcanonical initializer、型別bridge、静的評価・defect cache・外部opaque identity・所有値・GC rootを実装した。
- 外部opaque型と具体化された外部関数、export付きexternalのwrapper、Borrowed/Owned/StringBuffer、型付き成功/失敗の外部ABIを接続した。
- Embeddedの6標準module/23予約操作、GPIO/UART/I2C/SPI/Clock/Consoleを供給し、ユーザーmoduleによる偽装を拒否する。ISR入口は実際のadapter allowlistと到達するheap・動的call・再帰・中断・runtime const等で検査する。
- record updateを型検査・評価順付きで実装し、残余IR拒否、HostRef binding、破棄されたshow/handler/evidenceの副作用・解放を検証した。
- closure内のFiber失敗辺を新arenaへ正しく移す。不正なSeqId再利用による走査のメモリ増大を、旧State/Fiber/Deferred/task統合試験で検出・修正した。

## 現在の証拠

[const全22旧試験](p7-const-corpus-compat.json)、[汎化const原文8試験](p7-generic-const-compat.json)、[Deferred/join/await全30旧試験](p7-concurrency-corpus-compat.json)、[Bytes残3旧試験](p7-legacy-bytes-compat.json)、[統合2旧試験](p7-integration-corpus-compat.json) は元本文・旧Options・観測を対応付け、生成Cを `-std=c11 -Wall -Werror -O0` / `-O2` で実行した。正常な所有値経路では1000反復・回収を検証する。GC anchor、外部化、private C probe等は各対応表で明記する。

旧release 0.24のtrait/helpers/collections全5正常sourceも、同期部分だけでなく全文を実行した。closure captureに隠れたhandleは旧仕様のruntime guardに従い、[可視型のsource拒否と区別](p7-scope-capture-contract.md)する。

[全source観測の対応表](../compiler-legacy-runtime-crosswalk.json)は174旧正常試験・169原文programを対象とし、未照合を明示する。受理やC object生成だけをruntime成功へ読み替えない。現在の個別試験・static check成功は、最終統合revisionの全workspace/Clippy/MSRV/独立環境検証の代用にしない。

## 残る終了条件

cleanup/取消/notes、aggregate/Cause/Scope、外部task/標準platform、String/closure等の旧観測を全件照合する。旧IR86試験・同期/中断文脈・schema fieldの対応を確認し、正本の428定義・226文脈・115関連試験・10追加条件を根拠付きで確定する。その後、最終sourceの全workspace・C両最適化・strict Clippy・実Rust1.85・本体読取り禁止の独立環境検証を実行する。
