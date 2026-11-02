# 旧forkのFlow IR・実装対応表

旧forkの対応範囲を独立コンパイラーへ移すための固定した参照資料です。新repoでの進捗と終了条件は [compiler-coverage.md](../../compiler-coverage.md)、更新する台帳は [compiler-coverage.json](../../compiler-coverage.json) に置きます。このdirectoryの表を新実装の対応表として書き換えません。

## 参照資料

| 資料 | 内容 | 機械処理用 |
| --- | --- | --- |
| [IRと実装・試験の対応表](flow-backend-inventory.md) | 定義ID → 担当実装・plan → 関連試験 | [JSON](flow-backend-inventory.json) |
| [Flow IR関連定義](flow-ir-inventory.md) | 型宣言・variant・field・定義hash | [JSON](flow-ir-inventory.json) |
| [実装箇所の抽出](flow-implementation-inventory.md) | lower・emit・拒否等の構文上の実装候補 | [JSON](flow-implementation-inventory.json) |
| [文脈別の対応](flow-coverage.md) | sync・suspend・common・inputごとの経路 | [JSON](flow-coverage.json) |

## 出自と変更

- 元リポジトリ: [aoi-netai/emela-emb-dev](https://github.com/aoi-netai/emela-emb-dev)
- 取得元の固定commit: `c34c0aeefdd966d854371b3110db00a47798751d`（参照checkout内の未公開commit）
- 実装リンク用の公開commit: [`6fb21ccb59da45ada607cba024641c32101c93dc`](https://github.com/aoi-netai/emela-emb-dev/commit/6fb21ccb59da45ada607cba024641c32101c93dc)
- 元directory: `crates/emela-codegen-emb/docs/generated/`
- ライセンス: Apache-2.0。元commitのLICENSEは、このrepoの [LICENSE](../../../LICENSE) と同一です。
- 元path・原本SHA-256・保存後SHA-256・変換内容: [provenance.json](provenance.json)

8ファイルを取得元commitのGit objectから取得しました。参照checkoutの未コミット変更・未追跡ファイルは含めていません。4つのJSONはbyte単位で原本と同一です。4つのMarkdownには固定参照である旨を追加し、旧ソースへ向かう相対リンクだけを公開commitのGitHub URLへ変更しました。表同士のリンクはこのdirectory内を参照します。コード例の `s.fork[Int](..)` 等は変更していません。

取得元commitはGitHubに未公開のため、リンクには公開済みの `6fb21cc` を使用します。8資料の原本と、リンク先199ファイルは両commitでGit blobが同一です。対応表の参照内容は変わりません。照合したpathとblob IDは `provenance.json` に記録しています。コンパイラー全体の移行基準は `c34c0ae` のままであり、この2つのcommit全体が同一という意味ではありません。

JSON内のpath・行番号・symbol・hashは旧forkの情報です。新repoのローカル実装への参照ではありません。新実装とその試験は移行台帳の別欄へ記録します。

`flow-backend-inventory.json` の `definition_inventory_sha256` は、元の生成処理でJSONを再serializeした内容のhashです。インデント付きJSONファイルそのもののhashとは異なります。配布ファイルのbyte一致には `provenance.json` のhashを使います。

## 数え方

| 集計 | 件数 | 意味 |
| --- | ---: | --- |
| 全定義 | 428 | variant 195 + 共有宣言43 + field 190 |
| variant | 195 | 一次監査168 + 追加metadata 27 |
| 文脈別の行 | 226 | common 85 + sync 58 + suspend 58 + input 25 |
| 関連試験 | 115 | 対応表に関連付けられた異なる試験。旧compilerの全試験数ではない |

文脈別表は一次監査168項目を扱い、追加metadata 27項目は含みません。旧表の `candidate_found` 206行と `rejection_evidence` 20行は構文上の証拠です。新実装が動いたことを表しません。`review_status: unreviewed` もそのまま保存しています。

backend表の入力拒否24項目と文脈別表の拒否20行は異なる集計です。旧Stageの4 variantは、後者で入力経路の候補として記録されています。また、assert・表示等をFlow前に変換するための拒否を、source機能の未対応と解釈してはいけません。詳細は [終了条件](../../compiler-coverage.md) に定義します。

これらは参照用データです。Emela本体のcrate・CLI・ソース・監査ツールをビルドや必須試験へ追加するものではありません。

## 取り込み時の確認

2026-11-02に、8ファイルの原本・保存後hash、4 JSONのbyte一致、定義・実装一覧が参照する旧ソースのhash、backend表が記録する定義一覧のcanonical JSON hashを確認しました。移行台帳の428定義・226文脈・115試験についても、旧資料とキー集合が一致し、重複・欠落がないことを確認しています。

これは参照資料の取り込み確認です。新compilerによる旧機能の実装・C実行確認ではなく、移行台帳は全件 `pending` から開始します。
