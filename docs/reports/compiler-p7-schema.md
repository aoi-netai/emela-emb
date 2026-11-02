# P7の独立IRと旧schemaの意味の対応

このレビューは、旧対応表のschemaを機能数として数えず、旧fieldの意味が新しいどの入力・変換・計画へ移ったかを確認するためのもの。固定旧revisionは `c34c0aeefdd966d854371b3110db00a47798751d`。P6確定 `a686172` とP7の各feature commitを参照する。最終統合試験の結果は [P7検証報告](compiler-p7.md) に記録する。

## 定義・名義・外部境界

| 旧schemaとfield | 独立表現と保持する意味 | 検査・観測 |
| --- | --- | --- |
| `Module.core / functions / error_shows` | `contract/ir.rs::Program` が所有するDefinitions・Function arena・具体error表示関数の対応。Core/LIRを借用せず、frontendから構築した値を受け取る | catalog/ID検証、generic error表示、sourceからの失敗とC診断 |
| `Module.stage`、`Stage` | 外部からFlow/Heap stageを指定するAPIを廃止。typed input→LoweredModule→PlannedModuleの内部境界と各変換が状態を所有する | 入力側のHeap専用runtime・既成landing残余拒否。stage表現の不要性と所有権・cleanupの意味を区別する |
| `EnumDef.name / params / variants / error / external` | 同名の独自EnumDef。具体型引数でnominal instanceを構築し、error payloadの型を区別する。externalはpointer ABIの名義で、constructor/field/patternを公開しない | P2 data/traits/failures、P7 external type・record update、独立IRのgeneric field |
| `VariantDef.name / fields / tys`、`VariantFields` | positional/named fieldの順序・field名・型を保持。UnitをC表現から省いてもlogical field位置を保つ | P7独立IRのnamed payload、Unit/Int64 tuple・projection・nullary/payload tag、P2 source match |
| `CtorRef.enum_id / variant` | arenaの定義IDとvariant index。構築・判別・fieldアクセス時に名義とindexを検証する | 不正ID/field/type拒否、generic fieldとinvalid tagのC観測 |
| `EffectDef.name / decl / args / ops / label_of` | 元effectの宣言identity、具体args、operation順、label由来を独立catalogで保持。labelを区別したevidence keyと共通runtime handler ABIを分離 | P4 labels/generic effect/module identity、P6 Scope keyのrow消去 |
| `OpDef.name / type_params / params / ret / fails / suspend` | operationの順序と具体引数・結果型、失敗/中断可否。未具体化の型や不適合なoperation数・ABIを拒否 | P4 handler、P5同期operation制約とresume、P6 Scope ABI拒否 |
| `External.bindings`、`ExternalBinding.target / host`、`HostRef`、`HostModule` | C bindingのtarget、GlobalまたはNamed/File/Bundled import metadataを保持。C symbol選択は独立host_refs解析で確定する | P7 host metadata4種→同じC symbol、同期/task adapter、外部export wrapper。Emela本体のmodule loaderには依存しない |

`Module` の旧 `compile_flow` 参照は、新しい `backend::compile_program`→`lower::lower_program` を正しい入口として参照する必要がある。既存台帳の旧symbolが残る5recordはこの関係を根拠に更新する。

## 型・集合・関数

| 旧schemaとfield | 独立表現と保持する意味 | 検査・観測 |
| --- | --- | --- |
| `Type` | 独自TypeTableのinterned ID。primitive・tuple・List・nominal Enum・Cell・Fn・Handler・Evidence・集合を保持。frontendの未具体化型をbackendへ持ち込まない | 型catalog検証、具体化と同一型の統合、P2/P3のsource/C型境界 |
| `FnSets.errors / effects` | backendの集合TypeId。sourceはfrontend InvocationSets/row solverで包含・等値・汎化を検査してから具体化する | P4 hidden/opaque row・Immediate、P6 Cause row aliasとScope集合制約 |
| `SetSummary.fails / ev / suspends / params` | 具体集合の失敗・evidence・中断ABIと名義引数を保持。Asyncはevidence不要、通常effectはoperation/evidenceを持つ | P5 Async/Clock、P6 Scope、P7 constのerror/effect family交差 |
| `FnType.params / ret / fails / ev / suspends / sets` | 値型・関数値の具体署名を保持。Unit引数のC消去、Never結果拡大、失敗/evidence/中断ABIの拡大を検査 | P4/P5のFnRef/Closure/Coerce/Apply、P6 child boxing、P7 const callback |
| `Function.name / params / ret / body / vars / stmts / seqs / labels` | 独自Functionと型別Arena。hintは可読性の補助で、ID/型/def-use・sequenceの制御を別に検証する | 未定義ID・非支配変数・sequence cycle・引数/結果不一致の拒否、C評価順・分岐・再帰 |
| `Function.exported / external` | `pub`と独立した明示C rootとforeign binding。export_symbolは追加した明示C alias。外部exportはprivate externalへ呼ぶ通常wrapperへ変換 | P1/P7の公開名・衝突・entry・ISR・external wrapper |
| `Function.constant` | source const identityを保持し、静的評価またはruntime cacheへ接続。型別instanceとcanonical familyは追加metadataで結ぶ | P2 const、P7 canonical root固定点・単一初期化・defect cache・自己参照retry |
| `Function.fails / suspends` | 具体failure/frame ABI。sourceのerror/effect集合から失敗・中断可能性を確定し、許可された中断callだけをframe graphへ渡す | P4 failure/effect row、P5 task/resume、P6 fork evidence/失敗/cleanup、同期callerのwait拒否 |
| `Function.env` | 既成envを持つ入力は拒否。独立closure変換がlifted関数とClosureMetadataの環境ID・capture表を生成し、capture型をcatalogへ再対応する | P7入力契約の拒否、P4 capture所有権、独立IRのclosure/function試験 |
| `Function.defects` | 旧のcached boolは入力として要求せず、reachable primitive/builtin/runtime/termを独立planが走査してRuntimeRequirements.defectsを算出する | P1 assertion/defect、P2数値変換、結果を捨てたdefect、P6 attemptのdefect即時停止 |

`FnId / VarId / StmtId / SeqId / LabelId` はそれぞれ異なる `la_arena::Idx<T>`。欠落IDを別arenaの偶然同じ整数で代用しない。`VarData.hint / repr / ty`、`LabelData.hint`、`Repr` は独自contractで保持し、logical Unit値とC storageの有無を区別する。P6のUnit→Any/fork/Deferredがこの区別を実行で確認する。

## body graphと後段metadata

| 旧schema | 意味の対応 | 検査・観測 |
| --- | --- | --- |
| `Seq.stmts / term`、`Stmt`、`Term` | ordered statementと終端を独自graphに保持。If/Switch/Loop/Block/Protect、Return/Fail/Break/Recur/Defectを検証・正規化する。Term::Unwindの内部variantは残るが独立lowerの入力としては許可せず、失敗・取消のunwindはProtect/control/frameの経路で扱う | source短絡・評価順・失敗escape、末尾再帰、cleanupを越える不正制御、取消とdefectの別経路 |
| `Case.body / key`、`CaseKey`、`SwitchOn` | label/Int/Int64/enum tagとcase sequenceの対応を保持。exhaustive指定だけで不正tagを成功扱いしない | P7独立IRのliteral switch・payload/nullary tag・NoMatch hook、P2 source match |
| `Operand`、`Const`、`PrimTy`、`PrimOp` | 変数参照とliteral、primitive計算を区別。Int/Int64 wrap・shift/除算のdefect・Float bits・String長を維持 | P1/P2の数値端値・NaN・負shift・NUL・Unicode、P7 C境界Float bits |
| `Value`、`Callee`、`OnFail` | Direct/Indirect/Op callと失敗辺、tuple/enum/list/cell/closure/evidence/Fiber操作を独自入力として検証 | 同期/中断を分けたP2〜P6 source/Cと独自IR。旧source builtin8はlowerしてから後段へ渡す |
| `Closure.body / captures / params / suspends` | closure変換が環境型・capture binding・family metadataを生成。source入力に既成のclosure layoutを要求しない | P4返却closure/cycle、P5中断所有値、P7 const closure・canonical family |
| `Suspend.saved / state`、`Stmt::Let.on_suspend / on_defect` | callerが保存変数/state/landingを指定する旧Heap入力は拒否。独自suspend変換・control/frame planが実際のlive値と中断位置を生成する | P5生存値・再帰frame・pool不足・typed task。P7残余入力拒否を別に確認 |
| `RuntimeOp` | ShowString/DebugPrint/Retire/Hold/Unhold等のruntime意味と、旧Heap専用残余入力を区別する | 表示/retire/ISR制御のC観測、結果破棄ShowStringの容量defect、Heap専用6種のnative/suspend入力拒否 |
| `FiberOp` | Scope/Join/Attempt/Interrupt/Await/Deferred/Causeのtyped操作を独立validatorとruntimeへ接続 | P6正常・失敗・取消・suspend cleanup・所有値、P7 source corpusで組合せを照合 |

旧 `Term::Defect` の第2fieldはassertのsource位置を表す `Option<String>`。固定旧forkの `crates/emela-codegen-emb/src/compiler/lower/adapter.rs::adapt_term` も `Term::Defect(value0, _)` としてこのfieldを組み込みbackendへ渡さない。新Termもmessage operandだけを保持し、旧組み込みtargetの観測を維持する。これはdefect landingとは別であり、残余 `Stmt::Let.on_defect` の入力拒否と混同しない。defectはboard hookで即時停止し、cleanup unwindしない。

closure内の `Value::Fiber.on_fail` も、Callと同様にHandleのerror/bodyとBreakのlabelを収集・remapする。元arenaのIDをlift先へ残さず、sourceのState/Fiber/Deferred統合試験と、IDをずらした独立IRで制御辺の保存を確認する。

このレビューだけではvariant/contextの実行成功を示さない。schema fieldを共用する場合も、正本の各record/contextには対応するsourceまたはIR試験・最終実行報告を残す。表現を廃止したStage/Heap入力の対象外判定を、source機能・所有権・cleanupの対象外判定へ広げない。
