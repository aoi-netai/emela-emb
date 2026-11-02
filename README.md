# emela-emb

EmelaソースからC、ヘッダー、artifact manifestを生成する、組み込み向けの独立コンパイラーです。字句解析・構文解析・検査・C生成をこのリポジトリが所有します。Emela本体のcrate、CLI、ソース、core/std、Pomeはビルド・生成に必要ありません。

## 最初の入力

```emel
@export(c, "answer")
pub fn answer() -> Int {
  42
}
```

P1では複数module、内部関数、scalar型、演算、分岐、自己末尾再帰、assert、同期externalを扱います。正確な構文、仕様の参照版、未対応の機能は [対応仕様](docs/language-subset.md) を参照してください。Emela全体の互換性・Tier 2適合は対象外です。

## ビルドと生成

Rustのstable toolchain（edition 2024対応）、rustfmt、Clippyを使用します。通常のRust依存はCargo.lockで固定します。C実行試験にはhostのC11 compiler（`cc`）も必須です。

```sh
cargo build --workspace --locked
cargo run --locked -p emela-emb-cli -- build examples/minimal/main.emel --out-dir generated
```

`generated/` へC、公開・board用ヘッダー、artifact manifestなど、compilerが返す生成物をすべて保存します。出力先には新規または空のディレクトリを指定してください。既存の生成物がある場合も上書きせずエラーにするため、再生成時は別の出力先を指定します。全生成物を一時ディレクトリで完成させてから出力先へ公開します。

診断は `入力ファイル:行:列: error[種別]: 理由` の形式で標準エラーへ表示します。入力・生成・保存の失敗時は非ゼロ終了し、既存の成果物を保持します。

複数ソースは明示した順に読み、最初の入力をentry moduleにします。`--entry` で入力名を選択でき、`--entry-function` はprogram起動用の関数名を指定します。

```sh
cargo run --locked -p emela-emb-cli -- build main.emel math.emel --entry main.emel --entry-function main --heap-bytes 0 --string-capacity 256 --out-dir generated
```

`--heap-bytes` は固定heapのbyte数（既定0）、`--string-capacity` は動的Stringのbyte容量（既定256、1〜512）です。容量はcompilerの `Options` と同じ入力経路へ渡します。ライブラリでは `compile_sources(&[Source::new(name, text)], entry_name, &Options)` を利用できます。filesystemや外部processはCLIと試験helperが担当します。

P1のsource/API・CLI試験は、生成したCを `cc -O0` と `cc -O2` の両方でコンパイル・リンク・実行します。C compilerの不在や実行失敗をskipしません。

現在の試験と独立環境での確認は [P1検証報告](docs/reports/compiler-p1.md)、最初の最小実装の履歴は [最小実験結果](docs/minimal-c-experiment.md) に記録しています。

旧fork相当への拡張は [対応表に基づく終了条件と移行台帳](docs/compiler-coverage.md) で追跡します。[旧Flow IR・実装対応表](docs/reference/legacy-flow/README.md) を固定した参照資料として同梱しています。

## 開発

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`emela-emb-compiler` はソースから生成物を返すライブラリ、`emela-emb-cli` は引数・ファイルI/O・診断表示を担当します。frontendからbackendへは独自の内部表現を渡し、backendはlower、plan、emitに分けます。

Apache License 2.0。
