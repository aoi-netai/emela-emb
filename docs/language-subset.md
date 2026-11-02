# 最小C生成で対応するEmelaの範囲

この実験版は、単一のEmelaソースを自前のlexer・parser・検査・C生成に通す独立したコンパイラーです。Emela本体のcrate、CLI、ソース、core/std、Pomeをビルドや実行に要求しません。ここに記載した範囲だけを受け入れ、その他の入力は診断します。

## 基準仕様

2026-11-02に確認したEmela仕様のrevisionは `c34c0aeefdd966d854371b3110db00a47798751d` です。これは仕様文書の出自を記録する値であり、取得する依存ではありません。

| 参照文書 | 参照section | この実験で使う規則 |
| --- | --- | --- |
| `spec/language/02-lexical.md` | `[lex.source]`、`[lex.name]`、`[lex.literal]`、`[lex.continuation]` | UTF-8、ASCIIの名前、整数、空白・改行 |
| `spec/language/03-keywords.md` | `[kw.reserved]`、`[kw.future]`、`[kw.annotation]` | 予約語、属性 |
| `spec/grammar.md` | `[grammar.lex]`、`[grammar.decl]`、`[grammar.expr]` | token、宣言、block末尾の式の値 |
| `spec/language/12-external.md` | `[ext.boundary]`、`[ext.export]` | C向け公開関数とIntのC表現 |
| `spec/targets/16-targets.md` | `[target.number]` | Intは符号付き32 bit整数 |
| `spec/targets/c.md` | `[target.c.export]` | export名を外部結合のC関数名とする |

仕様の全機能、既存backendのABI・artifact schema、組み込みtargetの適合を表明するものではありません。対応を追加するときは、規則・診断・試験を同じ変更で追加します。

## 受け入れるプログラム

```emel
@export(c, "answer")
pub fn answer() -> Int {
  42
}
```

- 入力は明示した単一のUTF-8ファイルです。ファイル内には、C向けexport属性付きの公開関数をちょうど1つ置きます。
- export属性は2引数の `@export(c, "C公開名")` だけです。
- 関数は `pub fn 名前() -> Int` とし、引数、型引数、failure、effect、suspendを持ちません。
- 本体は非負の10進整数リテラル1つです。その式の値が関数の戻り値になります。
- 整数の値は `0` 以上 `2147483647` 以下です。桁数にかかわらず範囲外は診断し、切り詰めやwrapを行いません。
- ソース上の関数名とC公開名は別です。`pub fn value() -> Int { 7 }` と `@export(c, "read_value")` の組合せでは、C関数名は `read_value` になります。
- 空ファイル、複数宣言、必要な記号の欠落、末尾の余分なtokenを成功として扱いません。

`42` や `answer` は固定しません。許容する名前と値を変えると、それに従ったCとヘッダーを生成します。

## 字句とレイアウト

空白はASCIIのspaceとtabです。改行はLFとCRLFを受け入れます。単独のCR、Unicodeの空白、未対応の文字には位置付きの診断を出します。

ソースの関数名はASCIIの小文字または `_` で始まり、以後にASCII英字・数字・`_` を使えます。単独の `_`、予約語、将来予約語は関数名にできません。`userId` のような綴りは字句上の名前として扱い、この実験版では命名規約のlintを行いません。

使用できない予約語は次のとおりです。

```text
fn type enum error const trait impl effect handler layer import
pub opaque suspend if else match fail escape use with assert self Self as
var return while loop let mut in where do try catch throw async yield defer macro mod
```

`fails`、`derive`、`init`、`release`、`of`、`default`、`by` は文脈依存の語であり、この実験の関数名の位置では通常の名前として扱います。

整数はASCIIの10進数字だけです。`-1` は整数リテラルではなく単項マイナス式なので対象外です。数値の区切り文字 `_`、`0x`・`0b`、Float、文字のsuffixを受け入れません。

属性の文字列はC公開名を読む目的だけに用います。文字列escape、補間、複数行文字列、一般のString値を受け入れません。

改行は常に空白と同じ意味ではありません。この部分集合では次の配置を守ります。

- export属性と `pub fn` 宣言は改行で分けます。間の空行は許容します。
- `pub fn 名前(` は同じ行に書きます。属性の引数括弧、関数の引数括弧の内側では改行できます。
- `->` は引数括弧の閉じ括弧と同じ行か、その次の継続行に書けます。`-> Int {` は同じ行に書きます。
- 本体の整数の前後、ファイルの先頭と末尾では空行とインデントを許容します。
- semicolonは使いません。コメントも今回の実験では未対応です。

例えば次のレイアウトは同じ関数を表します。

```emel

@export(
  c,
  "answer"
)

pub fn answer(
)
  -> Int { 42 }

```

## Cの公開名と生成物

C出力の対象はC11です。C公開名は `[A-Za-z_][A-Za-z0-9_]*` に従う空でないASCIIの識別子とし、次の名前は保守的に拒否します。

- 先頭が `_` の名前、C11の予約語、`main`。
- `emela_embedded` または `EMELA_EMBEDDED` で始まる名前。ヘッダーguardを含む生成側の名前を予約します。
- `int` または `uint` で始まり `_t` で終わる名前。`int32_t` など、`<stdint.h>` の型名との衝突を避けます。
- `INT`、`UINT`、`SIZE_`、`PTRDIFF_`、`SIG_ATOMIC_`、`WCHAR_`、`WINT_` で始まる名前。`INT32_MAX` など、`<stdint.h>` のmacroとの衝突を避けます。

これらは実験版の公開名の制約であり、Cの全標準ライブラリとの名前衝突を解決する仕組みではありません。Emelaの関数名の命名規則はC公開名には適用しないため、`TIM2_IRQHandler` のような公開名を使えます。

成果物は次の2ファイルです。

```text
emela_embedded.c
emela_embedded.h
```

ヘッダーは `<stdint.h>` をincludeし、`int32_t C公開名(void);` を宣言します。Cファイルはそのヘッダーをincludeして同じ型の関数を定義し、入力で指定された整数を返します。runtime、board hook、manifestは生成しません。

CLIは `emela-emb build <file> --out-dir <dir>` で起動します。compilerの診断にはソース範囲が付き、CLIはファイル名・行・列・理由を表示して非ゼロで終了します。入力の読込みや出力の保存に失敗した場合も非ゼロで終了します。

今回の完了地点はCとヘッダーの生成です。Cのコンパイル・リンク・実行、および実機の動作確認は次の実験で扱います。

## 未対応の機能

コメント、Unicode名、文字列escape・補間、数値の区切り文字・基数表記に加え、関数引数、複数関数、演算、分岐、ループ、呼出し、変数、import、Pome、汎化、一般の型推論、構造化データ、closure、effect、suspend、動的メモリ、emb-std、周辺機器、board、firmwareを対象に含めません。未対応入力を無視したり、ダミーのCへ変換したりしません。
