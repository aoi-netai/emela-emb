# P7 構造化control・Value・Fiber contextの照合提案

`p7-structural-context-proposal.json`はP6台帳 `a686172` の122 contextを扱う。既verified49件の根拠を維持し、未完73件を個別に照合して全122件をverified候補とする。common/input99と重複せず、root担当Stage5と合わせて元226 contextのID集合と完全一致することを確認した。正本台帳は変更しない。

control/call/fiberの74件は `p7-control-fiber-context-proposal.json`、Value48件は `ff3ac8f` の `p7-value-context-proposal.json` を査読して結合した。参照先関数と、付いたSHA-256が固定commitの内容に一致することも確認した。

- native/suspendは、その命令を含む関数のflagに基づく。async rootからnative helperを呼ぶだけの経路を、helper内命令のsuspend証拠へ置き換えない。
- Literal Switchはsource matchのIfで代用せず、`769241b`の実task待機後I32/I64 min/max/defaultを直接C観測した。Tag/Variantのsuspendは明示的なf.suspendsを持つDeferred poll/Complete/awaitの独自IRを使う。
- Term Recur/Stmt Loopは、明示的なsuspend Scope bodyで1slotのDeferredを1000回作成・破棄し、1000scopeで反復したIRへ対応する。Term Break/Blockは、実wait後のsource escapeと`Context::escape_expression`が同じ関数内へ生成するIRを追った。
- source List4命令は、実Clock待機を伴うmap callbackとfoldのRを標準source・pattern lowering・ABI射影まで追う。Tagはasync Cause-kindの3variant match、IsError/AsErrorは実wait後のtyped error escapeから同じ関数内の生成を確認する。
- Tuple/Getはnative/suspend両flagの直接alias試験、Runtime suspendはlate.suspendsをassertした後のHold挿入と実C notes観測に対応する。MakeClosure/EnvGet入力は残っている構造の具体的Unsupported拒否であり、N/Aにしない。
- native Scope/Join/Await/Interrupt/DeferredAwaitの境界は、まず同じtyped inputがsuspend callerで受理されることを確認し、caller flagだけを変えた拒否へ結び付ける。CauseRaiseのfailure edge/channel不足も両modeの直接診断へ対応する。

Unreachable直接到達は`931c3ce`、最小dynamic Defectは`81a4ef6`の追加IR/C証拠を使い、実際に踏まないtailだけで完了扱いにしない。全件の最終統合HEADによるworkspace/C両最適化/実MSRV/strict Clippy確認はrootで実施する必要がある。
