# 固定旧の正常source入力

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の正常期待174testに対応する169program/170本文を保存する。これは入力の保存であり、新compilerでの成功を示すものではない。

`manifest.json` の `cases` は旧testとprogramの対応、`programs` はlogical filename・entry・本文ID・SHA-256を持つ。`sources` のファイル名は本文hashであり、compilerへ渡すlogical filenameはmanifest側の `name` を使う。複数moduleのentryと名前解決を保持するためである。

本文は旧testの最終入力とbyte単位で一致する。include後の置換、concat、formatの12variant、条件付きのInt/String成功型も展開済み。sourceへC export注釈を暗黙追加していない。旧testのFlow後処理やGC/task adapterがある場合は `execution_configuration` を確認し、単純なsource実行と区別する。

再生成はrepo rootから行う。旧repoは不要。

```sh
python3 docs/tools/materialize_legacy_fixtures.py
python3 docs/tools/materialize_legacy_fixtures.py --check
```

出自と抽出根拠は `docs/compiler-legacy-source-corpus.json`、具体化reviewは `docs/tools/legacy_corpus_reviews.json`、説明は `docs/reports/compiler-p7-corpus.md` に保存している。compiler/C試験はこのdirectoryの保存済み本文を使い、旧repo・旧frontend・旧core/std bundleを読み込まない。
