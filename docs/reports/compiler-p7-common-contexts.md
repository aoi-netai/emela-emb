# P7 common/input contextの照合提案

`p7-common-context-proposal.json`はP6台帳 `a686172` の99 contextを扱う。Builtin 58、RuntimeOp 11、PrimOp 13、PrimTy 4、Repr 5、Const 6、Operand 2であり、backend担当の残127 contextと重複しない。Stageの5 contextはrootのschema境界説明に委ねる。

70既verifiedは根拠を維持し、関数symbolを明示して再照合した。未完29件は次の実観測と実装へ結び付け、全99件をverified候補とする。正本台帳の更新と最終統合HEADでの全体runはroot担当であり、このproposalだけから完了扱いにしない。

- 未変換source-only Builtin 8件とHeap-stage RuntimeOp 6件は、実在するtyped inputを検査前提に構築し、個別のUnsupported診断を確認する。source機能を削除したとの判定とは分ける。
- StringJoin/Splitはnative sourceの値観測に加え、`aeb7c53`の実task中断を跨ぐString/Listの保持・入力上書き・独立buffer・容量8・解放を用いる。
- ShowStringの未使用結果でも容量defectを保つこと、DebugPrintの正確なbytesと順序を、sourceまたは独自IRのC試験へ対応付ける。
- 数値比較6操作、NUL付きString primitive、参照表現、String定数、変数/定数operandは、動的public引数と境界値を持つ既存試験へ対応する。operandは実中断経路にも対応付ける。

各参照の実装path・関数symbol・試験関数symbolの実在を確認した。common/inputという元contextを、記載されていない型・caller modeの直積へ広げない。native/suspendのCFG・Value・Fiber等は別のcontext対応表で確認する。一般builtinの既存試験を重複作成せず、不足だったTerm::Unreachable直接観測はvariant提案から別途参照する。
