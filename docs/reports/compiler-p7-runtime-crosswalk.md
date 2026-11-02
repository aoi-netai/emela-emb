# 全正常 source の旧 C 観測対応

[crosswalk JSON](../compiler-legacy-runtime-crosswalk.json) は、固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の正常 source 174 試験・169 program を全件保持する。元本文・生成 variant の hash、旧 C 呼出しと実行設定、現在の試験候補・実行 report・担当・残観測を区別する。正本台帳の状態は変更しない。

| 観測の照合状況 | 旧試験数 |
| --- | ---: |
| 元本文の C 実行証拠あり | 34 |
| 元本文の runtime const ISR 拒否と別入力の静的実行を照合 | 1 |
| 既存のより広い source/C 試験が旧観測を包含 | 4 |
| P5 正本で source 置換済み | 7 |
| 旧試験が frontend の診断だけを観測 | 1 |
| 旧試験が compile と header artifact だけを観測 | 1 |
| C の旧観測との対応が未完了 | 126 |

const 22 試験は `f8ac2aa` と `406a5d6` へ、release の helpers/collections 2 試験は `8677ff1`、traits 3 試験は `b1a83f1` へ結ぶ。元本文の hash を再計算し、不要な body の書換えがないことを検証する。GC 用 Cell[Any] の IR anchor、overlap の Hold 挿入、child 数を調べる private C instrumentation は旧試験由来の明示 seam として残す。純 source のみの実行とは区別する。opaque const 2 試験の既存包含は、原本文そのものを再実行したことにはしない。

P6 lifecycle の元本文 4 入力は旧 6 試験に対応し、child FIFO/handleless/overlap の 4 入力とともに各 C 観測を保持する。これらは `cb56194` と個別 checkpoint の証拠であり、独立 final snapshot の完了は別途必要である。Bytes size/get の既存試験には `1b4ca67` で旧 ISR entry 指定を補った。

現在の未照合は Deferred 12、join/await 14、Cause/Attempt 10、cleanup/interruption/notes 34、aggregate/integration 14、Scope 7、Bytes 3、native external/task 5、suspend handler 5、再帰型 3、String/owned 4、Embedded/platform 4、closure/State/helpers 11。family は元 path による分担であり、複数の旧試験が同じ program を共有する。既存 C 試験との assertion 比較で包含を確認できれば、重複試験を追加しない。

[P6 到達 snapshot](../compiler-p6-source-reach.json) は `1086f6a` の測定を本文変更なしで保存した。raw 139 accepted / 30 rejected は当時の結果で、Scope row 修正や P7 統合後の現状ではない。具体 root の拒否のうち 57 は task adapter 未指定、12 は external ABI の Options 未適応であり、元 tool の P5 未接続という分類をそのまま言語不足には使わない。source 受理、C 生成、C object、link/runtime を別 field で維持し、到達測定だけから runtime 成功にはしない。凍結した P4 到達結果も上書きしない。

再生成は Python 標準 library、保存済み fixture/index、明示したこの repo の checkpoint Git object のみを読み、旧 checkout を必要としない。

```sh
python3 docs/tools/audit_source_runtime.py
python3 docs/tools/audit_source_runtime.py --check
```

review 入力は [legacy_source_runtime_reviews.json](../tools/legacy_source_runtime_reviews.json)。表は実行証拠を整理した途中結果であり、174 全体や P7 の完了判定ではない。
