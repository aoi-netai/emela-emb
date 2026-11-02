# 最小C生成の実験結果

本書は最小実験時点の履歴です。現在の対応範囲は [7段階計画](compiler-parity-plan.md)、検証結果は [P1報告](reports/compiler-p1.md) を参照してください。

実施日: 2026-11-02（JST）  
作業ブランチ: `experiment/minimal-c`  
検証した実装commit: `b56f84ce4c7bd297d322047ac69a0305eff74f0c`

## 結果

次の入力を、CLIから字句解析、構文解析、対応範囲・型検査、lower、plan、emitに通し、Cとヘッダーを生成できました。

```emel
@export(c, "answer")
pub fn answer() -> Int {
  42
}
```

```sh
cargo run --locked --offline -p emela-emb-cli -- build examples/minimal/main.emel --out-dir generated
```

`emela_embedded.c` の生成内容:

```c
#include "emela_embedded.h"

int32_t answer(void) {
    return 42;
}
```

`emela_embedded.h` は `<stdint.h>`、header guard、対応する `int32_t answer(void);` 宣言を持ちます。名前と整数値は入力から読み取ります。対応範囲は [language-subset.md](language-subset.md) に記載しています。

今回の完了地点はCとヘッダーの生成です。Cのコンパイル・リンク・実行確認は行っていません。

## 検証

Linux x86_64、Rust `1.99.1 (58cf8f9c3 2026-10-13)`、stable toolchainで確認しました。

| 検査 | 結果 |
| --- | --- |
| `cargo test --workspace --locked --offline` | 30件成功（compiler unit 10、compiler結合4、CLI unit 2、CLI結合14） |
| `cargo fmt --all --check` | 成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 成功、警告なし |
| `git diff --check` | 成功 |
| `cargo tree --workspace --locked --offline` | workspaceの2 crateのみ。外部Rustライブラリ・上流Emela crateなし |
| 実CLIによる入力例の生成 | C・ヘッダーが完全なsnapshotと一致 |
| 単独コピーでのビルド・試験・生成 | 元のEmela作業ディレクトリへのアクセスを禁止した状態で成功 |

試験では、ソース関数名とexport名の分離、別の公開名・整数、Int上限、先頭ゼロの10進正規化、対応する改行配置、CRLFを確認しています。字句不正、途中の入力、余分な宣言、型不適合、範囲外整数、C名衝突、未対応構文には位置付きの診断を返します。

CLIの試験には読込み・保存失敗、生成失敗時の既存成果物の保持、非空出力先・symlinkの拒否、一時ディレクトリの後始末、OS native pathの処理を含めています。出力先は新規または空のディレクトリに限定します。

## 本体に依存しないことの確認

コミット済みのリポジトリだけを `git archive` で `/tmp` に取り出しました。コピーにはEmela本体、core/std、Pome設定、外部Rustライブラリを含めていません。

Linux Landlock（ABI 7）で検証processと子processのファイルアクセスを制限し、元の `/home/aoi_nemui/emela` の読取りが `PermissionError` になることを確認しました。利用できるのはコピー、空のHOME・Cargoキャッシュ・一時領域、Rust toolchainとOSのツール類です。PATHにはEmela CLIを追加していません。

この環境で次の手順が成功しました。

1. Cargo metadataがコピー内の2 crateだけを指すことを確認。
2. `cargo build --workspace --locked --offline`。
3. `cargo test --workspace --locked --offline`（30件成功）。
4. 入力例からCLIでC・ヘッダーを生成し、snapshotと完全一致することを確認。

## 実装のコミット

| commit | 内容 |
| --- | --- |
| `4ba97da` | 独立workspace、入力例、対応仕様、ビルド設定 |
| `b78a832` | ソース範囲付きの字句解析と診断 |
| `ea493b7` | export関数の構文解析とAST |
| `2d5e5cc` | 対応する型・整数範囲・C公開名の検査 |
| `00e101f` | lower・plan・emitと公開compile API、snapshot試験 |
| `8d9db87` | CLI、位置付き診断、生成物の保存と結合試験 |
| `b56f84c` | 一時ディレクトリ名と出力先名の一致を防ぐ修正・試験 |

pushは行っていません。
