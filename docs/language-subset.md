# 独立コンパイラーが対応するEmelaの範囲

P1〜P5の基盤・同期型システム・固定heap・関数値・effect・中断と外部taskを記録する。P5は365試験・独立環境での検証を完了。字句解析、構文解析、module・名前解決、型検査、独自IR、C生成をこのrepoが所有し、Emela本体のcrate・CLI・checkout・core/std・Pomeを要求しない。[実装計画](compiler-parity-plan.md) と [終了条件](compiler-coverage.md) に従い、後続段階の機能を追加する。

基準仕様は旧forkの `c34c0aeefdd966d854371b3110db00a47798751d`。`spec/language/02-lexical.md`、`03-keywords.md`、`12-external.md`、`spec/grammar.md`、`spec/targets/16-targets.md`、`spec/targets/c.md` を参照する。これは仕様・採用コードの出自であり、取得する依存ではない。全Emela仕様・全target契約への適合を表明しない。

## ソースと関数

```emel
import Math.{add}

fn twice(n: Int) -> Int { add(n, n) }

@export(c, "answer")
pub fn answer() -> Int {
  value = twice(21)
  assert value == 42
  value
}
```

対応する `math.emel`:

```emel
pub fn add(a: Int, b: Int) -> Int { a + b }
```

- 複数関数、引数、明示した引数型、関数の戻り値推論、名前付き束縛、同名の再束縛、block、関数呼出しを扱う。
- `if`、算術・比較、単項マイナス・論理否定、`&&`・`||`、関数へのpipeを扱う。引数は左から右へ評価し、短絡演算の不要な辺は実行しない。
- 自己末尾呼出しを反復へ変換する。一般の同期呼出しはC stackを使う。相互再帰する関数は依存群をまとめて推論し、未確定の戻り型を汎化する。
- `assert 条件` はBoolを検査し、失敗するとboard defect hookへ渡して停止する。
- `@external(c, "C名")` を付けたbodyなしの関数宣言で同期scalar C関数を呼び出せる。明示した署名とC側の定義を一致させる。
- 空白はspace/tab、改行はLF/CRLF、コメントは `#` から行末。改行はtokenとして保持し、括弧内の継続とblock内の文を区別する。semicolonは使わない。
- 名前はASCII。Unicode識別子、未対応の宣言や式は位置付きで診断する。

moduleは明示したソース群から解決し、entryからimportで到達するファイルを解析する。例えば `math.emel` は `Math`、`utilities/number_ops.emel` は `Utilities.NumberOps` に対応する。`import Math`、`import Math.{add}` を利用できる。選択importでも `Math.add` のmodule名は残り、同一定義の再importを許可する。別moduleから見えるのは `pub` 宣言だけで、循環import・欠落module・衝突・非公開名の参照は診断する。入口ファイル自体の名前は既存CLIと互換に扱う。

## scalar型と数値

| 型 | 表現・用途 |
| --- | --- |
| `Int` | 符号付き32 bit、C `int32_t` |
| `Int64` | 符号付き64 bit、C `int64_t` |
| `Float` | binary64、C `double` |
| `Bool` | `True` / `False`、C境界は生成headerの `int32_t` |
| `Unit` / `()` | 値 `()`、C境界は `void` |
| `Never` | 正常に戻らない経路 |

整数は10進・`0x`・`0b` と桁間の `_` に対応する。先頭ゼロは10進として扱う。整数literalは文脈のInt/Int64範囲で検査し、範囲外を切り詰めない。負数は単項マイナス式なので、最小値は `Int.MIN` / `Int64.MIN` を用いる。最大値は `Int.MAX` / `Int64.MAX`。

Float literalは小数点を含む形と、その指数表記（例 `1.0e-2`）を扱う。`1e9` のように小数点のない指数表記は仕様に従い拒否する。

Int/Int64の演算overflowはwrapする。除算は0方向へ丸め、最小値を-1で割る場合もwrapする。除算・余りのゼロ除数、負のshift量はdefectとなり、計算結果を使わない場合も検査を残す。幅以上のshiftは旧backendの規則に従う。Floatの算術・比較はNaN等を含むbinary64の意味を維持する。

compilerが所有する `lib/core/` の宣言と型registryで、基本型・数値限界、Int/Int64のbit演算・shift、Int64のInt変換、FloatのInt変換を供給する。intrinsicは解決済みの宣言に結び付け、ユーザー関数の名前だけで選ばない。数値parse/formatとString APIも、このrepoが所有する標準sourceから同じfrontendを通す。

## データ型・型推論・trait

- tuple、`type`・`enum`・`error`・`opaque`、型引数、名前付きfieldの構築と読取りに対応する。opaqueの構築・field参照は定義module内に限定する。
- pattern束縛、constructor/tuple pattern、後置 `match`、guard、網羅性検査を扱う。const patternは静的な値による網羅性とruntime constの一度評価を分ける。
- 引数の型は明示し、戻り型は推論できる。明示型パラメーターに加え、相互再帰群の検査後に未確定の戻り型を汎化する。具体化は呼出し先と型引数ごとに共有する。
- `trait`・`impl`・default・supertrait・derive、Eq/Ord/Show、method固有の型パラメーターと制約を扱う。deriveはfieldのcustom implを呼び、構造比較で置き換えない。FloatにOrdは供給しない。
- `Trait.method(value, ...)` と `value.method(...)` を扱う。後者はreceiverの型の定義moduleの公開関数と、呼出し元で可視のtraitから解決する。receiverは一度だけ評価し、曖昧な候補を拒否する。
- 未使用の宣言・generic body・field型の不正も検査する。汎化した関数をそのままC exportすることはできない。

## 失敗・String・const

`error` の値を `fail` し、`fails` の集合を検査・推論する。`escape` のpatternが共同で網羅したerrorを集合から除き、部分回復は残りの失敗を伝播する。明示した戻り型でfailsを省略した関数は、未回復の失敗を許可しない。失敗するtrait methodとgenericなerror型も扱う。戻り型・fails省略時は、error payloadの型変数も相互再帰群の汎化へ含め、呼出しごとに具体化する。escapeの最外patternにはerror constructorを使い、単独の `_`・変数・constは許可しない。error/effect集合はhidden/opaque row・具体instance・包含・汎化とImmediate制約を扱う。

String literal、補間、view比較、append、検索・trim、byte数・Unicode18の書記素数、code pointからの生成を扱う。NULを含む値は長さで渡す。Int/Int64/Float/Boolの変換と表記、Int.checked_add、Int/Float/Bool.parse、Optionの同期APIを供給する。parseの失敗は元のinputを持つParseErrorを返す。固定旧版にはInt64.parseはない。Stringのchars/code_points/split/concat/joinとASCII case変換も扱う。

動的Stringの容量は明示した設定で計画し、超過時はboard defect hookへ渡す。戻り値の保存先は生成headerのcaller buffer契約に従う。Stringを保持する失敗payloadやOption等が所有化を要求するときは固定heap容量を指定する。システムmallocは使用しない。

単相constを静的評価し、静的に確定できないものは遅延初期化・cacheする。未使用constは初期化せず、初期化順・一度だけの実行・defect cacheを維持する。汎化constの型別bridgeはP7の対象。

`panic`・`todo`・`assert` はdefect hookで即時停止する。`inspect` はShowを要求し、Stringを引用する。`dbg` は入力を一度だけ評価し、custom Showを優先して残りのfieldを再帰表示し、元の値を返す。defect時のcleanup unwindは組み込みtargetの契約に含めない。

## コレクションと固定heap

- Listの構築・spread・空/非空pattern、要素型の推論、Eq/Ord/Show、同期の検索・連結・反転・range・zip・take/drop・安定sortを扱う。要素のcustom traitを通常の呼出しとして利用する。
- Std.Bytesは所有者付きbufferと借用viewを扱う。List変換、slice/concat、UTF-8 encode/decode、hex、unsignedな比較・表示を供給する。長さはu32で、String容量から独立する。
- Std.Map/Std.Setは固定版の平衡木とopaque境界を維持し、構築・検索・更新・削除・列挙・unionとcustom keyを扱う。callbackを受け取るAPIも扱い、順序・短絡・安定sort・失敗途中の解放を維持する。

所有String・List・Bytes・再帰値には `--heap-bytes` / `Options::with_heap_bytes` で固定容量を指定する。借用値が返却や保存により元の寿命を超える場合は所有化し、公開Cへ渡った所有値は生成headerのrelease契約に従う。深い再帰値の解放は反復処理を使う。容量不足はdefectとなり、結果を使わない場合も必要な容量検査・解放を残す。

Unicode18の全853vectorについて、書記素のbyte境界・code point列・join再構成を確認した。Anyの箱化は失敗集合の内部表現と独自IRで扱い、公開sourceの動的型構文は追加しない。

## 関数値・effect・共有状態

関数・constructor・trait method・intrinsicの参照、lambda、間接呼出し、高階関数を扱う。lambda引数はconstructor patternも使える。関数型のfailure/effect集合を検査し、Never戻り型や失敗・evidence ABIの拡大を具体化へ反映する。返却closureはcaptureしたString/Bytes等を所有化し、生成元の呼出し終了後も使用できる。

`effect`・`use`・`handler`・`with`・`layer`、label/default、init/releaseを扱う。operationはhandler構築時のevidenceを保持する。withは取得に成功したhandlerを逆順にretire・releaseし、必要なcleanupを終えてから外側のescapeへ失敗を渡す。未使用layerの不正も検査する。

capabilityの直接返却・引数・data格納・値型注釈や、withの寿命を越える保持closureを拒否する。その場で実行するcallbackは許可する。`Std.State.run` は共有Cellを持つhandlerへ展開し、返却closureから共有状態を更新できる。公開Cell型は設けない。Cell/closure/Anyの循環は最後の外部rootを解放してから回収する。

`Std.Console` のSystemConsole/TestConsoleを供給する。SystemConsoleのString境界には `Options::with_external_adapter` の明示設定を使う。同期callbackを持つOption/List/Map/Setも標準sourceから同じfrontendを通す。Clockの中断・再開は次節のP5対応を参照。

## 中断・外部task・Clock

Asyncとeffect集合から関数・operation・関数値の中断可能性を判定する。Asyncはruntime要求で、通常effectのevidenceと区別する。外部suspend関数の呼出し・参照はhandler operation内に限る。同期operationでの実際の中断と、非ImmediateなeffectをImmediateへ制約する操作を拒否する。resumeを保持するhandlerの構築はAsyncを要求する。

外部taskには `Options::with_task_adapter` の明示設定を使う。frame内の生存値、所有String・closure・型付き成功/失敗を保持する。完了通知はqueueへ積み、producerと解放callbackはmain側で一度だけ実行する。古い通知・重複通知・二重resume・固定pool不足を検査する。成功/失敗中にcleanupが中断する場合は、cleanup完了後に元結果を配送する。

Std.Duration/Std.Instant/Std.ClockとSystemClock/TestClockを供給する。State[Instant]・List.mapのeffect多相callbackも中断・再開できる。Clock.nowだけを呼ぶ通常helperもClockの中断可能性を保持する。同期operation内からnowを直接呼ぶ場合は同期ABIを使う。

## C公開境界と生成物

**`pub` はEmela内の可視性、`@export(c, "名前")` はC公開を指定する。** `pub` だけでCの公開関数にはならない。内部関数にはexport属性を付ける必要はない。到達したimport先の明示exportも保持する。exportとexternalの衝突、重複export、不正なC公開名を拒否する。C公開関数と起動entryへ未供給のeffectを持ち出すことも拒否する。

`Options::with_entry` / CLIの `--entry-function` は起動用関数を選ぶ。これはC公開属性とは別の指定で、非公開関数も通常の起動経路から呼べる。`@export` はISR安全性を保証せず、割り込みvectorへの接続も行わない。ISR制約のsource側接続はP7で確認する。

C11のソース、公開・board用header、artifact schema 2のmanifestとexports情報をメモリ上の `Output.files` で返す。`Output.c_source` / `header` も便利な参照として維持する。公開署名は生成headerを正本にし、内部symbol・整形・出力順の過去版とのbyte一致は約束しない。停止・起動等で要求するhookは生成headerとmanifestに従い、host試験ではC mockを接続する。

公開名は非予約のASCII C識別子に限定し、先頭 `_`、C予約語、`main`、生成runtimeの予約名・prefix、標準整数型・macroと衝突する名前を拒否する。`TIM2_IRQHandler` のようなC公開名は利用できる。

## API・CLIと検証

`compile(&str)` は単一ソースの便利入口。`compile_sources(&[Source::new(name, text)], entry_source, &Options)` はソース群・入口・設定を受け取る。compilerはfilesystem・環境・外部processを読まず、CLIが入出力を担当する。

```sh
emela-emb build main.emel math.emel --entry main.emel --out-dir generated
```

`--entry-function`、`--heap-bytes`、`--string-capacity`、`--task-adapter C名=adapter名`、`--fiber-capacity`、`--frame-capacity`、`--frame-bytes`、`--task-capacity`、`--event-capacity` で明示した設定を渡せる。新規または空の出力先に全生成物を一時ディレクトリから公開し、既存の非空ディレクトリ・ファイル・symlinkを保護する。終了コードは成功/helpが0、入力・生成・保存失敗が1、引数エラーが2。

[P1検証報告](reports/compiler-p1.md)、[P2検証報告](reports/compiler-p2.md)、[P3検証報告](reports/compiler-p3.md)、[P4検証報告](reports/compiler-p4.md)、[P5検証報告](reports/compiler-p5.md) にsource/C・CLI・独立環境の確認を記録する。生成Cは `-O0` / `-O2` の両方でコンパイル・リンク・実行する。試験環境のC compiler不在を成功として扱わない。

## 後続段階

P6の構造化並行処理と取消、P7の残るABI・ISR・標準bridge・汎化constは、ここで記録したsource対応には含めない。backendのコードが採用済みでも、sourceからの接続・拒否試験・C実行を確認するまでは未完了として台帳へ記録する。

Pomeによる取得、物理BSP、firmware build/flash、実機確認、LSP・formatterは今回のcompiler移行の範囲外。
