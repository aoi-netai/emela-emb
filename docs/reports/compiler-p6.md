# P6構造化並行処理の検証報告（実装中）

P5確定版 `13cd313` を基点に、Scope/Fiber/Deferred/Causeと取消・cleanupを接続する。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。現時点では独自IRの基盤を統合しており、標準source接続と最終検証は継続中。

## 統合した基盤

- Scopeのcallback失敗ABI、fork戻り値のAny表現、Scope evidence keyと型付きFiber/Deferredの名義を正規化する。
- Join/Attempt、Deferredの一度だけの完了・poll・FIFO再開、typed Causeの分類・表示・再送出を検証する。空Causeの再送出はsource用のprivate landingへ接続する。
- 子fiber取消は入れ子のcleanupを逆順に実行し、中断するcleanupが終わるまで元の結果を配送しない。
- Scope呼出地点のevidenceを値として保持し、Provideの明示baseから子callbackのevidenceを構成できる。sourceのeffect・capability・capture走査へbaseを接続する。
- UnitのAny格納、Unitを返すchildのjoinとDeferredを扱う。Cに無効なvoid localを生成せず、副作用を一度だけ実行する。
- 名義型のrow引数をaliasする際も集合の等値制約を保持し、Causeの具体error集合を失わない。Hold/Unholdは引数・結果なしのUnit操作に限り、ISRから拒否する。

標準Fiber/Deferredの前提として、外部opaque型の宣言構文とmetadataを先に供給する。一般の外部C ABIとその検証はP7へ残す。

## 中間検証

source revision `d4b9b23` の `cargo test -p emela-emb-compiler --lib --locked --offline` は213 pass、失敗/ignore 0。独自IRの正常C経路は `cc -std=c11 -Wall -Werror -O0` / `-O2` で成功し、所有権を伴う経路は1000回反復・全heap回収を確認する。ログは `.local/experiment/p6-unit-boxing-lib.log`。

直前の `90eb956` は209 pass。strict ClippyはsourceでEvidence/Fiberをまだ構築していないためdead_codeで失敗した。抑制属性は追加せず、標準sourceの合成経路を接続した後に再確認する。

## 残る完了条件

Std.Fiber/Deferred/Causeのtrusted source合成、Scopeの戻り型と集合制約を正式branchへ統合する。sourceから正常・失敗・取消・中断cleanup・所有値の再開と不正入力の拒否を確認し、最終sourceの全workspace・strict Clippy・実Rust1.85・独立環境検証を行う。台帳はそれらの証拠に基づいて更新し、独自IRの成功だけでsource対応を完了扱いにしない。
