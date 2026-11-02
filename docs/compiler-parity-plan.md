# 旧fork相当の独立コンパイラーへ到達する7段階の実装計画

作成日: 2026-11-02（JST）

状態: 2026-11-03、P1〜P7完了。固定旧forkの428定義・226文脈・115関連試験・追加10要件を全件判定し、独立環境の607試験を完走した。[P7検証報告](reports/compiler-p7.md) を参照。最小C生成のS0〜S6も完了済み。

P1は [検証報告](reports/compiler-p1.md) の66試験、P2は [検証報告](reports/compiler-p2.md) の201試験、P3は [検証報告](reports/compiler-p3.md) の231試験、P4は [検証報告](reports/compiler-p4.md) の335試験・C両最適化実行・独立環境確認を完了。GitHubでは `gh stack` により段階ごとのPRを管理する。

本書の旧試験名は計画作成時に選定した移植対象の参照である。実装後の観測・実行結果は各段階の検証報告と移行台帳を正本とする。

2026-11-02追記: 旧Flow IR・実装対応表の取り込みと移行台帳の初期化を先行して実施した。[固定参照](reference/legacy-flow/README.md)、[終了条件](compiler-coverage.md)、[新repoの台帳](compiler-coverage.json) をGit管理対象に置く。台帳の初期状態は全件未判定。

P6は [検証報告](reports/compiler-p6.md) の398試験・C両最適化・独立環境検証を完了した。旧全source/IR観測の照合はP7で判定する。

## 1. 目標と進め方

旧forkのコンパイラー実装・runtime・試験を活用し、`emela-emb` を7段階で旧fork相当のコンパイラーへ拡張する。最小実験を繰り返す段階から、既存の機能群を取り込み、独自frontendから接続して完成させる段階へ移る。

旧forkと現在の実装は、ともに `lower → plan → emit` を持つ。旧forkの `contract`・`semantics`・`plan`・`emit` は本体の型から分離済みであり、この構成と実装を再利用する。独立済みのbackend一式をP1で取り込み、P1〜P7を**ソースから利用できる機能群の完成単位**にする。7段階を、旧ファイルを完全に7分割してコピーする単位にはしない。

最終目標は、旧forkで動作する対象ソースを自前の解析・型検査・loweringに通し、同等のC・runtime・公開境界を生成して実行できること。旧forkの手組みIRだけで検証される機能も、独自IRで対応を維持する。IR試験だけの成功をソース対応として数えない。

本体のcrate・CLI・checkout・core/std bundleへの依存を設けず、本体を変更しない。frontend一式のコピーは行わず、名前解決・型推論・effect検査・具体化は新repoが所有する。旧backend固有のコード、runtime、試験入力・期待値は出自とライセンスを保存して再利用する。通常のRustライブラリはCargoで管理してよい。

この7段階の到達点は「基準旧forkのコンパイラーと同等」である。全Emela仕様への対応は、その対応表と既知差分を土台として継続する。ボード製品全体の移植とは区別する。

## 2. 比較基準と対象範囲

### 2.1 固定する旧fork

| 項目 | 基準 |
| --- | --- |
| 参照checkout | `/home/aoi_nemui/emela/tmp_branch/ecosystem` |
| revision | `c34c0aeefdd966d854371b3110db00a47798751d` |
| コンパイラー | `crates/emela-codegen-emb` の当該commitのtracked tree |
| 構成の根拠 | `src/compiler/README.md`、`src/contract/`、`src/compiler/{lower,plan,emit}/` |
| 機能・制約の根拠 | `docs/implementation.md`、`docs/artifact-contract.md` |
| 対応確認の材料 | `tests/`、`docs/generated/flow-backend-inventory.*`、関連inventory |
| 導入順の参考 | `stack/codegen-emb-01-base`〜`07-completion`。コードの採用元は上記revisionに統一 |

参照checkoutには未コミット変更・未追跡ファイルがある。移植には `git show <revision>:<path>` または指定revisionのarchiveを使い、それらを混入させない。移植元の更新が必要になった場合は基準revisionと差分を記録する。旧checkoutを新repoのビルド・必須試験から参照しない。

旧機能・試験の一覧は `docs/reference/legacy-flow/` へ固定済み。新repoの進捗は `docs/compiler-coverage.json` で管理する。旧inventoryは構文上の実装入口や試験の存在を示す資料であり、その件数を意味論の対応率として扱わない。

### 2.2 この計画に含むもの

- ソースの字句・構文解析、module/import、名前解決、型・trait・error/effect・capability検査、具体化。
- 必要なcore/stdの宣言・intrinsic・共有処理の互換実装。旧compiler試験が使う標準APIを一覧化し、各段階で供給する。
- 独自IR、検証器、lower・plan・emit、生成runtime、設定値と制約診断。
- export、同期external、非同期task、所有値のC ABI、ISR到達性検査。
- `Embedded.*` の宣言・共有handler・C bridge、必要サービス・hookの宣言、manifest生成。
- 生成Cのhostでのコンパイル・リンク・実行と、外部処理・周辺機器のC mockによる検証。
- 既存CLIを複数ソース・設定・複数生成ファイルへ接続するための必要な拡張。

物理BSP、startup/vector/linker、toolchainの探索・配布、firmwareのbuild/flash、Renode・実機確認、保存済みartifactのplatform接続製品、Pomeの依存取得、LSP・formatterの移植は別計画とする。compilerが生成するplatform用header・要求サービスmetadataは今回に含める。

### 2.3 引き継ぐ組み込み契約

旧forkと同じラインを基準にするため、次を採用する。恒久的なtarget制約と、段階途中の未実装を対応表で分ける。

| 項目 | 採用する契約 |
| --- | --- |
| メモリ | システムmallocを使わず、設定した固定heap・機能別poolを使用。容量不足はdefect。heapを使わない処理も維持する |
| 再帰 | 同期再帰はC stack、suspend再帰はframe poolを使用。自己末尾呼出しのstack非消費を維持。再帰を一律禁止しない |
| defect | 旧forkと同じ即時停止。cleanup・参照解放をせず停止hookへ渡す。通常Emelaのdefect unwindとの差を文書化する |
| 失敗・取消 | 回復可能なfailsと取消では必要なcleanup・解放を行う。cleanupがsuspendする場合も完了を待つ |
| 実行文脈 | scheduler・heapの通常操作はmain側。複数thread/coreやcallbackからの任意の再入を許可する契約にはしない |
| ISR | exportとは別に入口を指定し、到達するheap・参照管理・再帰・suspend・runtime const初期化・動的call等を検査。完了通知はqueueへ送りmain側で再開 |
| C境界 | scalar同期external、所有権を明示する複合値adapter、suspend用task adapter、公開値のretain/releaseを維持 |
| 生成物 | 最終的に旧C ABI 2・artifact schema 2の契約を採用。署名・layoutは生成headerを正本とする |

内部C symbol・整形・生成順序のbyte一致は到達条件にしない。指定したexport名、公開ABI、観測できる値・副作用・失敗・所有権を比較する。旧forkでも保証されない生成間のlayout/export番号一致は新たに約束しない。上流Core/Flowを引数に取るRust APIの互換性は不要。

## 3. 再利用の境界と構成

| 旧forkの範囲 | 新repoでの扱い |
| --- | --- |
| `src/contract/` | 独自ID・型・関数・制御・ABI・段階間データを採用し、new frontendの出力へ接続 |
| `src/semantics/` | 数値、Unicode、C命名、型・ABI照会等を再利用 |
| `src/compiler/plan/` | layout・所有権・String buffer・heap・runtime・suspend・ISR等の計画を再利用 |
| `src/compiler/emit/`、`src/writer.rs` | C/header出力・機能別renderer・固定C runtimeを再利用 |
| `src/compiler/lower/` | 変換単位とアルゴリズムを維持し、`emela_core`・`emela_lir`を扱う部分を独自IRへ付け替える |
| `src/input/upstream/` | upstream adapterを持ち込まず、必要な検証・const評価等を独自入力に実装する |
| `tests/support/frontend.rs` | 独自frontendを呼ぶhelperへ置換。旧resolver/types/coreをdev-dependencyにも残さない |
| ソース・C harness・期待値・IR fixture | 機能ごとに再利用し、入口とIDを新compilerへ接続 |

plan/emitには横断的な依存があるため、独立済みのコードとC資産はP1でまとまって取り込む。未接続の機能はfrontend/検証器で診断し、対応しているように見せるダミーのloweringは作らない。後段のコードが存在することと、その機能の完成を区別する。

処理経路:

```text
CLIがソース群・設定を読む
  → lex / parse
  → module収集・名前解決
  → 型・error/effect・trait・capability検査
  → 必要な具体化と独自の型付きIR
  → lower（正規化、closure/handler変換、const、suspend入力）
  → LoweredModule
  → plan（配置・所有権・資源・ABI）
  → PlannedModule
  → emit
  → Output（C・header・manifest・exports）
  → CLIが保存 / 試験helperがCをコンパイル・実行
```

旧 `contract::ir::Program`、`LoweredModule`、`PlannedModule` を土台にする。lower前の型付きIRはfrontendが所有し、未具体化の型・集合やsource位置を保持する。旧backendの表現で足りる箇所は再利用し、必要なlower前の情報だけを補う。外部向けのCore/Flow交換形式やシリアライズ契約を新設しない。

`LoweredModule` のclosure・const・suspend・到達性metadataは、lowerの各変換が生成する。frontendから一つの巨大な処理で埋めない。constの元宣言IDはfrontendから追跡できる形にし、旧upstream metadataへの依存を置き換える。

予定構成（2 crateを維持）:

```text
crates/emela-emb-compiler/
├── src/
│   ├── lib.rs
│   ├── diagnostic.rs
│   ├── frontend/
│   │   ├── lexer.rs / parser.rs / ast.rs
│   │   ├── modules.rs / resolve.rs
│   │   ├── types/       # 値型・error/effect集合・trait・capability
│   │   ├── specialize.rs
│   │   └── ir.rs        # lower前の型付き表現
│   ├── contract/       # 独自backend IR・段階間データ・Options・Output
│   ├── backend/
│   │   ├── lower/
│   │   ├── plan/
│   │   └── emit/       # 機能別出力とruntime/を含む
│   ├── semantics/
│   ├── sources.rs      # このrepoが所有する標準機能の供給
│   └── writer.rs
├── lib/
│   ├── core/
│   ├── std/
│   └── embedded/
└── tests/
    ├── compat/         # ソース・期待値・C harness。台帳はdocs/compiler-coverage.json
    ├── backend/        # 独自IRによる試験
    └── support/
```

compilerの入口はメモリ上のソース群・entry・Optionsを受け取り、生成物を返す純粋なAPIへ拡張する。現在の単一ソース `compile(&str)` は既定設定の便利入口として残す。具体的なAPI名・型はP1で確定する。filesystem・環境・外部processはCLIまたは試験側が担当する。

## 4. 7段階の全体像

旧stackの依存順を使い、各段階に独自frontendと標準機能の作業を組み込む。

| 段階 | 完成させる機能群 | 到達点 |
| --- | --- | --- |
| P1 Base | backend採用、module/関数/名前解決/型の基盤、scalar・制御 | 複数ソースの数値プログラムをC生成・実行できる |
| P2 Sync | データ型、型推論・汎化・trait、失敗、String・数値、const | 同期の型付きプログラムを広く扱える |
| P3 Memory | 固定heap、List/Bytes/再帰値/Any、所有権 | 動的なデータを固定容量で扱い、解放できる |
| P4 Effects | closure、高階関数、effect/handler/layer、Cell/State | Emelaの関数値・effect・共有状態が動く |
| P5 Suspend | 中断・再開、frame、task、型付き外部完了 | 非同期の外部処理から安全に再開できる |
| P6 Concurrency | Scope/Fiber/Deferred/Cause、取消・cleanup | 構造化並行処理が所有権と整合して動く |
| P7 Completion | 汎化const・型導出・C ABI・ISR・標準bridge、全件照合 | 基準旧forkのcompiler範囲に未移植機能を残さない |

P1〜P7は依存順のマイルストーンであり、各段階を1commitにしない。段階間で必要な型・metadataは先に導入できるが、その機能の完了は対応する試験で判定する。

## 5. 各段階の実装内容と完了条件

### P1. Base — 独立backendの採用とscalar・制御の完成

**実装するもの**

- 取り込み済みの `docs/reference/legacy-flow/provenance.json` と `docs/compiler-coverage.json` を利用し、担当段階・新実装・試験を記録する。コード再利用時のrevision・LICENSE・ファイル一覧は `docs/migration-source.md` に追加する。
- 独立したcontract・semantics・plan・emit・runtime・Writerを採用し、最小実装のbackendを統合する。通常のRust依存を明示し、上流crate・workspace継承・外部へのincludeを除く。
- コメント・改行・名前・数値・演算子の優先順位、複数関数、引数、束縛、呼出し、`if`、自己末尾再帰を実装。module/import、公開範囲、shadowingの基盤を作る。
- 値型・error/effect情報を分離できる型表現、定義ID・ソース位置、明示署名を中心にした型検査と基本的な推論、独自IR検証器を導入する。
- Int/Int64/Float/Bool/Unit/Never、基本演算・比較・短絡評価を接続。overflow、除算・shift境界、評価順を旧実装と合わせる。
- sourceの `assert` を解析・型検査し、失敗をdefect hookへ送る。以降のsource回帰試験をここから利用可能にする。
- exportと内部関数を分離し、scalar同期external、entry選択、到達性を接続。既存の最小入力とC公開名を維持する。
- CLIへ明示したソース群・entry・Optionsと複数生成ファイルの保存を接続する。Pomeによる取得は必要にしない。将来の容量設定・ISR指定も同じ入力経路へ追加する。
- Preludeの基本型と数値intrinsicを独立供給し、C実行helperを用意する。builtinは名前文字列だけでなく、解決済みの宣言IDで識別する。

**再利用する主な試験**: `tests/emit.rs` の基本値・演算・direct call・if・loop/recur、`tests/external_scalar/`、`tests/exports.rs`。sourceの反復は末尾再帰等で表し、Emelaにないloop構文を追加しない。

**完了条件**: 複数moduleの関数呼出し、分岐、末尾再帰、数値境界を含むプログラムが新CLIからCへ変換され、hostで `-O0` / `-O2` の両方で実行できる。未定義名・型不一致・不正な引数・export衝突を位置付きで診断できる。独立backendの採用だけではP1完了にしない。

### P2. Sync — 同期データ・型システム・失敗・String

**実装するもの**

- tuple、`type`・`enum`・`error`・`opaque`、構築・field読取り、名前付き引数、pattern、後置matchと網羅性検査。
- 相互参照する定義群の型推論、top-level汎化・具体化、型パラメーター制約、`trait`・`impl`・default・supertrait・derive。同期の値型とerror集合を扱う。
- `fails`・`fail`・`escape`、失敗辺とcleanupの表現。通常の値と失敗値のABIを接続する。
- String literal/view・補間、容量を計画する動的String、数値変換・Float parse/format、Unicode・書記素処理の基盤。Listを返すAPIはP3で接続する。
- 単相constの静的評価とruntime初期化・cache・defect保存を接続。元宣言IDと具体化の対応はここから維持し、汎化constの型別bridge完成はP7で行う。
- Option、Eq/Ord/Show、数値・Stringの標準APIを供給する。高階関数・effectを必要とする具体化はP4以降へ対応付ける。

**再利用元**: `plan/{layouts,strings,const_runtime}.rs`、`emit/{layouts,string_runtime,numeric_runtime,float_to_string,float_parse}.rs`、失敗・Protectのrenderer、`semantics/unicode_grapheme.rs`。

**試験と完了条件**: tuple/enum・match・型推論・trait境界・失敗伝播・String容量・数値変換のsource試験と拒否試験が通る。`tests/release_0_24/traits.emel` から同期traitの入力と期待値を独立fixtureへ抽出し、`tests/string_append/`、`tests/string_comparison/`、関連const試験の同期部分を接続する。元のtraitsファイルはList・closure・Fiber/Causeも含むため、全体の実行はP6以降に行いP7で照合する。通常値だけでなく境界値、埋込みNUL、非ASCII、defect hookを確認する。

### P3. Memory — 固定heap・List・再帰データ・所有権

**実装するもの**

- List、Bytes、再帰データ、所有String、Any/EmelaErrorの表現と型情報を接続する。有限の型表現と再帰する辺の箱を扱う。
- 固定heap、参照の追加・解放、借用viewからの所有化、戻り値・分岐・失敗値の寿命、深い値の反復解放を接続する。
- cycle回収の基盤を導入し、Cell/closureを含む実際の循環はP4で完成させる。
- 結果を使わない場合も、必要な副作用・容量検査・defect・解放を残す。
- List、Std.Bytes、Std.Map、Std.Setのデータ操作を独立供給する。closure/effectを受け取るコンビネーターはP4の完了条件へ割り当てる。

**再利用元**: `plan/{heap,analysis,layouts}.rs`、heap/ownership生成、`emit/bytes_runtime.rs`、List/Bytes/Anyのrenderer、`semantics/type_representations.rs`。

**試験と完了条件**: `tests/owned/`、`tests/bytes/`、`tests/any_data/`、`tests/value_audit/`、`tests/builtin_audit/` の該当項目を移植する。固定容量の繰返し使用・再利用、容量不足defect、深い再帰値の解放、borrowed値を保持した後の寿命をC実行で確認する。Cの処理系mallocに逃がさない。

### P4. Effects — closure・effect・handler・共有状態

**実装するもの**

- 匿名関数、関数値、capture、高階関数、間接呼出し、関数値coerce、関数を返す関数。
- effect集合の推論・汎化、`effect`・`handler`・`use`・`with`・`layer`、handler要求・初期化順、capability/captureの逸出検査。
- 旧closure・handler変換を独自IRへ移し、静的/動的evidenceと環境の所有権を接続する。
- `init`/`release`、handler失効、共有Cell、Std.State、Cell/closure/Anyを経由するcycle回収。
- List/Map/Setの高階・effect多相API、Consoleや独自の同期effectの宣言・mock handlerを供給する。P4の実行試験はImmediateなeffectで揃える。Clockはsuspend operationを持つため、handlerの実行はP5で接続する。

**再利用元**: `lower/{closure,coerce,handler}.rs`、`plan/adapters.rs`、関数値・handler・evidence・Cellのrenderer。

**試験と完了条件**: captureした値が生成元の呼出し終了後も有効で、関数値と動的effectの組合せが実行できる。effect不足・trait制約違反・capability逸出を拒否する。`tests/cell/`、`tests/binding_metadata/`、`tests/builtin_audit/retire.rs` とclosure/evidenceの試験を接続し、環境・共有状態・cycleの解放を確認する。

### P5. Suspend — 中断・再開と外部task

**実装するもの**

- suspend関数・operation、Immediate制約、effect集合からの中断可能性判定、Async要求、同期/中断関数値の適合。
- lowerのsuspend変換、制御graph、生存値保存、frame配置、one-shot resume、suspend再帰・closure・handler・release。
- 最小の協調scheduler、固定fiber/frame/task/event領域、型付きtaskの引数・結果・失敗。
- 外部task adapter、producer/context lease、借用・所有値の扱い、完了通知queue、stale/重複通知・容量不足の検査。
- 待機・再開に必要なClock/Instant/Duration等の互換APIとC mockを供給する。Clockはnowだけを使う関数でも中断可能なeffectとして扱う。IR上のsuspend/failure/evidence情報を確定してbackendへ渡す。

**再利用元**: `lower/suspend.rs`、`plan/{suspend,task_values}.rs`、`contract/control.rs`、`emit/task_values.rs`、`emit/runtime/{scheduler,resume,task}.*` と関連資産。

**試験と完了条件**: `tests/suspend/`、`tests/task_values/`、`tests/scheduler_runtime.rs` の該当試験を移植する。外部完了後に正しい位置へ戻り、保持したmanaged値を使用・解放できる。複数回resume、古い通知、pool不足を検査する。producerと解放callbackはmain側で呼び、完了通知からその場でEmelaへ再入しない。

### P6. Concurrency — Scope・Fiber・取消・cleanup

**実装するもの**

- Scopeのerror/effect集合と包含制約、Fiber/Deferredの寿命とscope外への逸出検査、子へ供給するeffect。
- Scope/Fork/Join/Await/Interrupt、Deferred、Cause、失敗・取消の伝播。
- `scope_abi`・`fiber_normalize`の独自IR化、cleanupを含む制御経路、suspendするcleanupと取消の延期。
- 子fiber・完了値・handle・共有state・task contextの回収。完了通知と取消が重なる場合の一度だけの解放。
- Std.Fiber・Std.Deferred・Std.Causeと対応する標準コンビネーターを供給する。

**再利用元**: `lower/{scope_abi,fiber_normalize}.rs`、`semantics/{child,deferred}.rs`、`emit/features/fibers/`、`emit/runtime/{scope,child_wait,deferred,deferred_await,unwind}.*`。

**試験と完了条件**: `tests/fiber/`、`tests/deferred/` と所有権・Cell・taskの横断試験を接続する。正常終了・失敗・取消の各経路でcleanupと所有権を確認し、suspendするreleaseの終了まで元の結果を配送しない。scope逸出・不適合な集合を診断する。

### P7. Completion — 型・const・外部境界を完成し全件照合する

**実装するもの**

- 汎化constの元宣言ID、canonical値、型別bridge、単一初期化、defect cache、managed rootの保持を完成する。特殊化ごとの二重初期化を防ぐ。
- nested/generic/recursive型の構造Eq/Show、有限の型導出、Unit/Never・空Cause等の境界を完成する。
- 非scalar/fails付き同期external、external opaque、Borrowed/Owned/StringBuffer adapter、保持値のretain/release、C公開名とexport aliasを完成する。
- ISR入口からの到達性検査、外部symbolのISR保証指定、必要サービス・hook・artifact metadataを完成する。exportだけでISR安全と判定しない。
- `Embedded.*` の宣言・共有handler・GPIO/UART/I²C/SPI/Clock/Console bridgeを接続し、C mockで確認する。compiler側のentry条件・C ABI 2・schema 2を検証する。
- assertの詳細診断・テスト用宣言等、旧compilerのsource corpusが要求する残りの構文・診断・標準APIを照合する。test実行用entryはローカル試験helperで供給でき、製品のtest CLIは必須にしない。
- 全段階の機能台帳を照合し、未移植・仮実装・到達不能な実装を残さず、既知差分を確定する。

**再利用元**: `lower/{const_bridge,const_runtime,type_normalize,exports}.rs`、`plan/{const_bridge,derive_runtime,external,interrupts,services}.rs`、artifact/bindingsと対応するemit。

**試験と完了条件**: `tests/const_runtime/`、`tests/completion_types/`、`tests/sync_external/`、`tests/exports.rs`、`tests/artifact.rs`、`tests/embedded_peripherals/`、`tests/release_0_24/` とISR試験を接続する。第6節の全件判定を完了し、第7節の終了条件を満たす。

## 6. 試験・対応台帳・旧forkとの照合

### 6.1 台帳の単位

`docs/compiler-coverage.json` を更新する台帳とし、`docs/compiler-coverage.md` に終了条件・状態・証拠の記録方法を定義済み。428定義、226文脈、115関連試験を初期化しており、以後少なくとも次を記録する。

```text
旧機能ID / 基準revision / 旧実装・試験 / 担当段階
新frontend / 新lower-plan-emit / 必要な標準API
source試験 / 独自IR試験 / C実行試験 / 制約・不正入力の診断
状態 / 既知差分 / 対象外とする場合の根拠
```

状態は「未接続」「IR検証済み」「source実行検証済み」「制約として拒否を検証」「入力契約が存在しない」を区別する。sourceを受け入れる機能はsource実行まで確認して完了にする。frontendの型・effect・capability検査は、不正入力が通らない試験も必要とする。

旧Core/Flow専用のstage・Heap IR禁止要素等は、新しい入力契約に存在しない場合だけ根拠付きで除外する。未対応の言語機能をこの区分で隠さない。旧正常sourceが失敗する場合は移植未完了である。

### 6.2 各段階で行う検証

1. 関連Rust試験、fmt、Clippy。段階間の逆依存・上流依存・compiler内I/Oも検査する。
2. ソースから新frontend・lower・plan・emitを通したC実行試験。`-O0` と `-O2` の両方で値・副作用・失敗・cleanupを確認する。
3. 独自IRによるbackend試験。sourceで作りにくい異常IR・型配置・特殊な制御経路も検証する。
4. 固定容量・所有権・ISR等、その段階で追加した制約の拒否試験と境界値試験。
5. CLIの生成・保存と診断。compilerの必須試験は新repoにある資産だけで完結させる。

C compilerを必須試験環境へ用意し、未導入やskipを成功として数えない。必要に応じて所有権の試験へsanitizerを追加する。旧forkの試験件数は参考に留め、同じ意味の試験を新しい構成へまとめ直せるようにする。

一つの旧fixtureが複数段階の機能を含む場合は、早期段階用に独立した入力・期待値を抽出し、元fixtureとの対応を台帳へ残す。元fixture全体も依存する最後の段階で実行する。未対応宣言の無視や、コンパイルできない部分の黙示的な除外で通さない。

本体や旧forkを使う差分実行は、移行時の任意の確認に限る。期待値・元ソース・設定・出自を新repoへ記録し、最終CIで本体を起動・取得・参照しない。Cの整形一致より、公開境界と実行結果を判定する。

### 6.3 特に落とさない横断条件

- 値を捨てても副作用・defect・容量検査・解放を維持する。
- 数値のwrap・shift・Floatの丸め/parse/format、UTF-8と書記素の意味を維持する。旧Unicode table・公式vectorの出自も保存する。
- borrowed値の保持、capture、failure、suspend、取消、cycle回収を組み合わせて検証する。
- 汎化constの単一初期化と、異なる具体化間で共有すべき値の同一性を維持する。
- taskのstale/重複通知・取消との競合・context解放を確認する。
- ISRの入口だけでなく、その呼出し先と外部symbolまで検査する。

### 6.4 旧source経路の既知制限

基準旧forkには、成長する再帰型のsource lowering、直接の関数値const、汎化effectのAsync具体化の一部に制限がある。基準表に具体例を固定し、一般の再帰型・const・effect全体を制限として扱わない。

新frontendで解消できるものは対応する段階で解消し、上積みとして記録する。旧forkでIR入力だけが成功する機能は独自IRで維持し、source到達の成否を別に記録する。P7後に残る旧source制限は、フルEmela対応へ向かう次の残件として明示する。これを理由に旧正常sourceの対応を減らさない。

## 7. 終了条件

- [ ] 固定した旧compilerの機能・標準API・試験が全件、移植済みまたは根拠のある対象外に分類されている。
- [ ] 取り込み済み対応表の428定義（195 variantを含む）・226文脈・115関連試験のID集合が台帳と一致し、重複・欠落・未判定を残していない。台帳の追加10条件も検証済みである。
- [ ] 旧forkで成功する対象source corpusが、自前frontendからC実行まで通る。
- [ ] 旧IR限定の機能が独自IR fixtureで動き、source対応との区別が記録されている。
- [ ] 固定heap、所有権、effect、suspend、並行処理、const、外部C、ISRの旧契約を検証している。
- [ ] 必要なcore/std・Embeddedの宣言・互換実装とintrinsic登録を新repoが所有している。
- [ ] C ABI 2・schema 2、公開header・要求hook・services・exportsの契約が成立する。
- [ ] 全対象のsource/C試験が `-O0` / `-O2` で成功し、不正入力・資源不足・不正ISRも検証している。
- [ ] 本体checkout・CLI・crate・core/std bundleへのビルド・生成・必須試験の依存がない。
- [ ] 本体を参照できない環境で、新repoだけのビルド・C生成・コンパイル・実行が成功する。
- [ ] 既知のsource制限と、board/platform製品として別途必要な作業が記録されている。

この条件を満たした時点で「旧fork相当の独立コンパイラー実装完了」とする。全Emelaソースの無条件受入れや、BSP・flash・実機までの完了とは表明しない。

## 8. ブランチ・commit・並行作業

段階ごとの作業ブランチは `feat/compiler-01-base`〜`feat/compiler-07-completion` を候補とし、直前段階の確定した成果を基点にする。最小実装のPRが未mergeの場合も、その成果を失わない基点を選ぶ。今回の実装ではユーザーの明示依頼に従い `gh stack` で段階ごとのPRを作成し、必要なpushを行う。mergeは行わない。

各段階内では機能単位で日本語commitを作る。例えばP1は、再利用元と対応台帳、独立contract/semanticsの採用、plan/emit/runtimeの採用、frontendの名前解決、型・制御、source接続とC実行、CLI/APIの接続を分ける。まとまった既存コードの採用commitには出自を記録し、その後の適応変更と区別する。途中commitもbuild可能にし、追跡可能な粒度を保つ。

並行作業は、段階の契約型を先に共有してから、frontend、lower接続、plan/emit/runtime適応、試験・標準機能に分担する。契約型・生成物・台帳の変更は担当を一つに決めて衝突を避ける。機能追加ごとに仕様・対応表・必要な試験を同じ作業単位で更新する。

P1完了後は小さな入力例の採否を毎回決め直さず、この7段階と台帳を基準に実装を進める。段階の終了時に実行結果・残件・次段階へ渡す契約を記録する。pushはユーザーの明示指示がある場合だけ行う。

## 9. 参照資料

- [最小C生成の検証結果](minimal-c-experiment.md)
- ローカル調査記録 `.local/plan/IMPLEMENTATION_HISTORY.md` — 旧Flow連携方針は採用せず、導入順とcommit粒度を参考にする。Git管理対象の計画・終了条件は本書と対応台帳を正本とする。
- 旧compilerの構成: 旧forkの `crates/emela-codegen-emb/src/compiler/README.md`（第2節の固定revision）
- 旧compilerの実装範囲: 旧forkの `crates/emela-codegen-emb/docs/implementation.md`（第2節の固定revision）
- 旧生成物の契約: 旧forkの `crates/emela-codegen-emb/docs/artifact-contract.md`（第2節の固定revision）
- 旧frontend試験helper: 旧forkの `crates/emela-codegen-emb/tests/support/frontend.rs`（第2節の固定revision）
- [取り込み済みの旧機能・試験inventory](reference/legacy-flow/flow-backend-inventory.md)
- [対応表に基づく終了条件](compiler-coverage.md) と [新repoの移行台帳](compiler-coverage.json)

旧forkの参照は移植元の説明であり、ビルド・試験の依存ではない。移植元は第2節のrevisionで確定する。本書・固定参照・台帳・終了条件をGit管理し、実装時にはコードの出自・target制約と対応試験も追加する。
