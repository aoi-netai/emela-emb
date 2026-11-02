# P7の旧IR期待値と実行contextの追加証拠

正本は編集せず、`p7-ir-context-proposal.json`へvariant/contextの候補を出す。固定旧は`c34c0aeefdd966d854371b3110db00a47798751d`。

- retireはdirect/evidence/coerce/tuple/argumentの5 aliasをnative/suspend双方で走らせ、同じhandler名を含むafter-release panicを観測する。fatal hookで停止する各caseは旧と同じ1実行である。
- Deferredはcapacity1の同じScope内で1000回構築し、unused destinationとdstNoneを両方確認する。外側1000回のScope完了後にもheapが空になる。
- Scope/Join/Await/Interrupt/DeferredAwaitは適合する型・引数を持つsuspending callerでlowerが成功し、callerのsuspendsだけをfalseへ変えると専用診断で拒否する。Cause.raiseはmissing edgeとinfallible propagationを各modeで拒否する。Hold/Unholdはdirect/nested ISR到達とglobal unwind runtime有無の8組を拒否する。
- StringSplit/Joinはsyncの既存source観測と分け、suspend IRで実task待機を跨ぐ。入力NUL/UTF8 view、'/'でjoinした結果を保持したまま復帰後に':'で再joinし、容量8で双方の内容・非alias・所有返却・heap解放を各1000回確認する。
- cleanupの中で別の'7' payloadをcatchしてから待機しても、元の所有'42' payloadが保持されることを既存P5 IRへ追加した。外側handlerへの公開はcleanup完了後である。
- 定数Float.round(-2.5)=-3、定数append("a","é")の3byte結果とruntime helper省略、Float定数1e-6を含むappendの19byte上限を直接IRで確認する。共通entryは新しいplatform start hookを経由し、容量2/64/512で1000回呼ぶ。公開関数は2byte caller bufferへ直接返す。
- Anyを含むtupleをerror payloadへ入れた間接failure callは、旧と同じBreak→Any成功/失敗union→IsError→AsErrorまたはCoerce→Int42の形で、旧2048byte heapの1000回を走らせる。Grow[A]のfieldがfn()->Grow[List[A]]となる再帰は、exported identityによるaggregate往復、callback投影、間接呼出しとEnd tagを旧4096byte heapで各1000回確認する。

Cは全case `-std=c11 -Wall -Wextra -Werror -O0/-O2`、省略なし。旧IRのhint名はVarIdの同一性を意味しないため、旧FnBuilderが新規割当するsuccess/error/outには別のVarIdを保持する。
