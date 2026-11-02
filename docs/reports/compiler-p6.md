# P6構造化並行処理の検証報告（実装中）

P5確定版 `13cd313` を基点に、Scope/Fiber/Deferred/Causeと取消・cleanupを接続する。固定旧forkは `c34c0aeefdd966d854371b3110db00a47798751d`。独自IR基盤と標準source接続を統合し、取消・combinatorのsource回帰と最終検証を継続中。

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

直前の `90eb956` は209 pass。source接続前のstrict ClippyはEvidence/Fiber未構築のdead_codeで失敗したが、抑制属性を追加せず標準sourceを接続して解消した。

`1086f6a` はStd.Fiber/Deferred/Causeの固定source、名義型のrow kind推論、Scope省略推論・子Scope供給、Cause/Exitのphantom row ABI射影を接続する。`p6_concurrency.rs` の2試験は通常のRust test profileで成功した。Scope/Fork/Join/Deferred、Unit child/Deferred、Attempt/Cause分類・表示・再送出の7exportをC両最適化・各1000回の全heap回収で確認し、Scope戻り型のhandle逸出と集合kind等7不正sourceを拒否した。

同revisionのstrict Clippy全target・実Rust1.85全target check・fmt/diffは成功、警告0。全workspace回帰は386 pass、失敗/ignore 0で成功した。Rust側は `CARGO_PROFILE_TEST_OPT_LEVEL=1` を使う。C側の必須両最適化は同じ。ログは `.local/experiment/p6-source-{initial,clippy,msrv}.log` と `p6-workspace-initial.log`。

## 残る完了条件

sourceから取消・中断cleanup・lexical provider・入れ子Scope・標準combinatorの回帰を追加し、最終sourceの全workspace・strict Clippy・実Rust1.85・独立環境検証を行う。台帳はそれらの証拠に基づいて更新し、独自IRの成功だけでsource対応を完了扱いにしない。

## 追加したsource回帰

`26cb8d0`のsource combinator試験は、Scope呼出地点のevidenceとfork地点のevidenceの区別、入れ子Scopeからの外側Scope利用、both/race/all/mapのString/List所有値をC両最適化・1000回の全heap回収で確認した。`dd5104e`の2試験は、attempt内のdefectが即時停止しreleaseを走らせないことと、先行するtyped failureではreleaseを行うことを検証する。停止後にしか使わない同期cleanup helperをCへ出さず、strict Cの未使用関数警告も解消した。

`0f9a3ca`はScope集合をchildごとの包含関係として推論し、pure childで提供集合を空へ固定しない。原本文を保存した4 sourceの回帰が成功し、直前の169プログラム監査でMissingTagだった15入力もすべて受理された。監査のC rootは旧Optionsを再現していないため、この15件の受理をC実行成功へ読み替えない。

`888b821`は旧4入力のchild await/join FIFO、失敗したhandleなしchildの回収、中断するsibling cleanup、取消通知と所有String、複数失敗noteの順序をC両最適化で各1000回検証する。[入力と観測の対応](p6-child-waiters-compat.json)にhashと旧試験名を記録した。overlapだけは旧試験と同じsource後のHold挿入を必要とし、純sourceの経路と区別する。

closing-ready・入れ子取消・interrupt待機のsource/C回帰を追加中。完了済みchildの未使用handleが親の最初の待機時点まで残る差分を検出しており、旧期待値どおり回収する修正を完了してからP6最終検証を行う。この時点ではP6を完了扱いにしない。
