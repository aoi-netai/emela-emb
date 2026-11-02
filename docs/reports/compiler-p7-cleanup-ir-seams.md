# P7 cleanup 12件の原source後IR適応とC観測

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の12試験を、保存済みの原source bytesと元のC期待値から実行した。fixture/harness資料は `49db59e`、Protect適応の共通helperは `da721f2` である。実行記録は `p7-cleanup-ir-seam-compat.json`、旧path/hash/option/適応手順を含む全資料は `p7-cleanup-source-seams.json` に残す。正本台帳は変更しない。

これらは旧試験自身が正常source loweringの後にIRを書き換えていた経路である。原sourceに明示C exportのみを追加し、通常のmodule collect・const expand・check・specialize・function ABI projection・lowerを完了してから、対応する旧IR適応を行う。pure sourceだけのC実行とは区別する。

| 旧適応 | 新試験の処理 |
| --- | --- |
| `protect_cleanup_call` | 共通 `p7_source_seams.rs` を再利用。末尾cleanup callをProtectへ移し、必要なEvExtend/EvLookup prefixを元の順でhoistする。Yield結果はfreshな同型変数を経由して保持。適用数1をassert |
| `hold_late` | 旧notes/overlap/payload helperと同じくsource関数lateの先頭へ、dstなし・引数なしのRuntime Holdを1個挿入。適用数1をassert |
| `external_request` | source requestが既にfails+suspendsであることをassertし、bodyを外して外部C host_requestへ替える。引数・戻り型・失敗/中断flagを保持。適用数1をassert |
| `private_C_cancel` | 旧helperどおり生成Cへprivate test_cancelを追記し、指定fiberのcancelled flagを立てる |

masked cleanup中のjoin、cleanupからのnested scope close・Deferred wait・self interrupt、取消されたscopeのfirst failureとordered notes、共有/rethrowされたCause、String/Listを含むerror payloadの外部producer/release/cancelが、元のC assertionで成功した。外部errorのC型名とtagは生成headerから解決し、宣言順に依存した固定値を埋め込まない。

12試験すべてがC11 `-Wall -Wextra -Werror -O0/-O2` でcompile/link/実行に成功した。frameは旧512/768/1536 bytes、heapは旧4096/8192 bytes、fiber/frame/task/event/Deferred poolも記録した旧設定を維持した。容量の増加はない。

正常11件は各最適化で1000回反復し、各回のroot/返却値解放後にheap空を確認する。masked cleanupの旧harnessはheap検査関数を宣言するだけだったため、旧assertionを保持したままroot release直後のheap空assertionを1個追加し、再実行した。残る `note_display_checks_the_callers_remaining_capacity` は故意のStringCapacity fatal hookで各最適化1回終了する。Causeを生存させたまま容量defectを検査する経路なので、defect後のheap空や1000回の正常反復とは表記しない。

moduleは `src/backend/lower/p7_cleanup_source_seam_tests.rs`、C assetsは `tests/compat/p7/cleanup/`。通常試験は旧repoや `/tmp` の資料を読み込まない。fixtureの元SHAをreport生成時に保存値と照合した。12件のfocused実行、追加heap assertionの単独実行、workspace全target strict Clippy、実Rust1.85.0全target check、fmt/diffが成功した。
