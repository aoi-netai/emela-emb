# P7 record update

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の `spec/language/05-data.md:44-55`、`spec/language/14-dynamics.md`、`crates/emela-core/src/lower.rs:3070-3138` を参照し、`User(name: "b", ..user)` を独立frontendへ接続した。

type/errorとfields付きhandlerを更新できる。enumはvariantが1個でも対象外で、関数・local・任意式をcalleeにした更新も拒否する。unknown nameは名前解決の診断を保つ。更新はnamed fieldのみ、重複・未知field・末尾以外のbaseを拒否する。opaque境界と、handler構築を同じhandlerのinit内に限定する既存規則も適用する。

結果の型引数はbaseから独立に推論し、引き継ぐfieldの型だけを制約へ加える。`Pair[Int, Bool]` のfirstをStringへ替えて `Pair[String, Bool]` を作れる。更新しないfieldと矛盾する型引数は拒否する。ASTにはUpdateとproduct/enum判別を追加し、typed IRは既存のBlock/Make/Field/Localへ展開する。1variant enumのfieldを直接読めていた判定もproduct判別へ直した。

更新する右辺を記載順で1回ずつ、その後baseを1回だけ評価する。baseの型は先に調べるが、実行順は合成Blockで保持する。固定旧lowerと同じく、変数参照を保持するためだけの強制所有化は加えていない。評価中に外部Cが借用inputを破壊しても古い内容を保つという追加保証は導入しない。Cへ返す所有データの寿命と解放は既存のABI契約に従う。

`tests/p7_record_updates.rs` はsource経由で次を確認する。

- 複数更新の記載順→baseの順序、baseの一度だけ評価、generic型引数変更、全field更新と差分なしの更新、error更新、handler init内の更新。
- 更新右辺の失敗とbase失敗、Neverのbase、既に作ったString/Listと失敗payloadの解放。
- 更新Stringと引き継いだListをCへ返し、元inputの上書きと元Listの解放後にも内容が残ること、1000反復後のheap全解放。
- opaque内部の正常更新と外部拒否、invalid callee、field誤記・重複・位置引数、1variant enumの直接field accessとupdate拒否、capabilityのdata格納拒否。

C実行は `-std=c11 -Wall -Werror -O0/-O2` の両方を必須にし、host compiler不在時もskipしない。旧repoの参照は通常試験へ残していない。

検証: 新source 3test、compiler unit 180test、P4 capability 1/handler 3/State 2testが成功。compiler全targetのstrict Clippy、実Rust 1.85.0でworkspace全target check、fmt/diff checkも終了0。P7全corpusのC実行完了を示す結果ではない。
