# 旧115試験のsource29件の最終対応案

固定旧revision `c34c0aeefdd966d854371b3110db00a47798751d` の115試験のうち、source経路を持つ29件を照合した。[JSON提案](p7-source29-legacy-test-proposal.json) は正本の `tests` への適用候補であり、正本自体は変更していない。残り86件は [独立IRの提案](p7-ir-legacy-test-proposal.json) が担当する。両集合に重複・不足はない。

29件すべてを `replaced` 候補とする。20件は保存した旧本文に明示C exportを追加して実行し、9件は既存の広域source試験が旧観測をすべて包含する。後者9件について原本全体を実行したとは主張しない。旧fixtureの29個のSHA、対応する精密test symbol、公開予定の統合祖先 `b0a367b` における全参照pathを確認した。

既に置換扱いだったBytesの7件は、小容量補強後の証拠を採用する。旧heapの8192/4096/256/128 byte、String容量1、70000 byte入力の旧262144 byte heapを区別し、大容量での成功だけから小容量での成功を推測しない。heaplessなsize/getは旧ISR entryも保持する。旧terminal HeapExhausted経路の正常returnや全解放は主張せず、全573 byteの旧Bytes debug corpusは永続PNG constの初回後heap使用量をbaselineとして照合する。

Deferred、join/await、aggregate、Cause、suspend cleanupは旧frame/pool/heap容量、通知順、NULL戻り先、所有値の返却後再使用、cleanup順を個別reportと現在のrunner/harnessで確認した。root取消を使う旧試験のprivate生成C seamを明記し、source後IR改変と混同しない。この29件にはsource後IR改変を必要とする試験はない。

canonical constの旧 `$emb_const_origins$v1$` 私有prefix/name検査は、新しい `canonical_const` metadataと依存constの固定点生成へ対応する。原本文のinteger/text双方42と初期化log計2回に加え、別試験でBASE/PAIR各1回を確認する。旧私有表現が新compilerに残るとは主張しない。

## schema178件の独立査読

[査読JSON](p7-final-schema-review.json) に全178 ID、53 path、89 path/symbol参照の検査を記録した。具体指摘は `Term::Defect.1` の説明誤り1件。旧第2fieldはlocal landingではなくassertの任意位置情報で、固定旧embedded adapterも破棄していた。rootは説明を訂正済みで、実装修正を要する新回帰は見つかっていない。Stageの7件だけを表現/APIのN/Aとする境界を確認した。

raw Switchの5variantはIR専用入力で、source matchはIfによる同意味へlowerされる。StmtLoopはfrontendが自己末尾callから生成するTerm::RecurをbackendがLoopで包むためsource由来の分類を維持する。これらの分類意見をrootへ共有済み。

## 検証

この文書変更では新たなruntime試験を追加・実行していない。実行証拠は [統合検証報告](p7-backend-proof-integration.json) の `fresh_workspace_validation` を参照し、source `d67dd4a` で単一fresh workspace607件成功、失敗・無視0、Cの `-O0/-O2`、strict Clippy、実Rust1.85を確認済み。元feature revisionは出自情報として残し、提案の実行参照には統合祖先 `b0a367b` を使う。最終正本・docsを含む独立archiveはroot/backendの担当であり、この査読でその完了を主張しない。
