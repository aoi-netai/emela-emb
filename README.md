# emela-emb

Emelaの小さな部分集合からCとヘッダーを生成する、組み込み向けコンパイラーの実験です。字句解析・構文解析・検査・C生成をこのリポジトリが所有します。Emela本体のcrate、CLI、ソース、core/std、Pomeはビルド・生成に必要ありません。

## 最初の入力

```emel
@export(c, "answer")
pub fn answer() -> Int {
  42
}
```

引数なしの公開関数1つと、非負の10進Intリテラル1つを扱います。正確な構文、仕様の参照版、未対応の機能は [対応仕様](docs/language-subset.md) を参照してください。Emela全体の互換性・Tier 2適合は対象外です。

## ビルドと生成

Rustのstable toolchain（edition 2024対応）、rustfmt、Clippyを使用します。外部Rustライブラリへの依存はありません。

```sh
cargo build --workspace --locked
cargo run --locked -p emela-emb-cli -- build examples/minimal/main.emel --out-dir generated
```

生成コマンドは実装中です。完成時には `generated/emela_embedded.c` と `generated/emela_embedded.h` を保存します。今回の実験の到達点はCとヘッダーの生成です。Cのコンパイル・リンク・実行は後続の実験で扱います。

## 開発

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

`emela-emb-compiler` はソースから生成物を返すライブラリ、`emela-emb-cli` は引数・ファイルI/O・診断表示を担当します。frontendからbackendへは独自の内部表現を渡し、backendはlower、plan、emitに分けます。

Apache License 2.0。
