# P7 compound式のCall結果を内部で使い終わるまで保持する

P7統合状態のworkspace回帰で、旧Bytes全文の `full_legacy_bytes_corpus_preserves_const_traits_and_every_debug_observation` が `main: variable is used before its definition` で失敗した。原sourceの `dbg(Bytes.size(data))` のような式が再現条件である。

P6の即時child回収用lowerは、非末尾source式の戻り変数を生成した新規Callを探し、そのdstをNoneへ変えていた。dbgは引数の値を返すため、この変数が内部のShow/DebugPrintから使われる場合にも定義を落としていた。独立IR入力validatorがこの未定義使用を検出した。

discardの対象を、そのsource式から新規生成した最後のstatementが戻り変数を生成するCallである場合に限定した。compound式の中間値と既存localを維持し、単独の未使用fork Callは引き続きdstNoneにして即時回収する。Drop/Dup入力や新しいpublic APIは追加しない。

次の既存回帰が修正前の失敗と修正後の観測を直接含むため、同じ処理を写したテストは追加しない。

- `tests/p7_legacy_bytes.rs` の3試験。原Bytes全文の47観測・出力573byte、旧heap131072/String容量512、1000回で保持heap量一定。suspend snapshotとtask producer/release/cancelも両最適化で成功。
- `tests/p6_scope_lifecycle.rs` の4試験。未使用child即回収、既存localの参照後再使用、closing-ready、nested cancel、interrupt FIFOを旧pool/heapで各1000回・heap空まで確認。

7試験はC11 `-Wall -Werror -O0/-O2` のcompile/link/実行に成功した。workspace全target strict Clippy、実Rust1.85.0全target check、fmt/diffも成功。元のworkspace実行はこのBytes targetで停止したため、後続targetの実行と最終standaloneは別記録で扱う。正本coverage台帳は編集しない。
