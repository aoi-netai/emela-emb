# P7 旧inline data/分岐IR試験の移植

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` のIR-only 9testを読み比べ、独立contractを使う2testへ再構成した。旧 `FnBuilder`/Core/Flow crateの依存は追加せず、通常試験から旧repoを参照しない。旧source正常174testのC実行には数えない。

移植先は `src/backend/lower/p7_data_tests.rs`（compiler crate内）。`legacy_inline_layouts_preserve_generic_fields_tags_and_unit_positions_at_the_c_boundary` は次の6testをまとめる。

| 固定旧test | 旧と新のC観測 |
| --- | --- |
| `tests/binding_metadata/mod.rs:61` `named_enum_fields_preserve_payload_order_in_generated_c` | Named(left:Int,right:Int64)のpayload f0=7/f1=42。宣言順を維持 |
| `tests/emit.rs:4456` `constructs_payload_and_unit_enum_variants_as_inline_values` | Someのtag=0/Int64 payload=42、Noneのtag=1 |
| `tests/emit.rs:4514` `reads_generic_enum_fields_from_inline_values` | 1variant Boxed[A]をInt64で構築し、generic field=73を読む |
| `tests/emit.rs:4562` `reads_tags_from_payload_and_nullary_enums` | payload enumとnullary enumのtag=1。新試験はpayload tag=0も観測 |
| `tests/emit.rs:4629` `constructs_inline_tuples_and_keeps_unit_field_positions` | (Int64,Unit,Bool)のC field f0=91/f2=True。Unitの位置を飛ばして詰めない |
| `tests/emit.rs:4656` `reads_tuple_fields_and_ignores_unit_projections` | Unit projectionは結果なし、index=2のBoolはTrue |

`legacy_literal_and_enum_switches_cover_numeric_extremes_and_reject_invalid_tags` は次の3testをまとめる。

| 固定旧test | 旧と新のC観測 |
| --- | --- |
| `tests/emit.rs:4697` `emits_and_runs_int32_and_int64_literal_switches` | Int MIN/0/other→11/22/33、Int64 MIN/MAX/other→44/55/66 |
| `tests/emit.rs:4760` `emits_and_runs_exhaustive_payload_enum_switch_and_defects_on_invalid_tag` | Some/None→71/82、不正tag=7→NoMatch kind、長さ8・`no match` message |
| `tests/emit.rs:4802` `emits_and_runs_switches_on_nullary_enum_representations` | scalar表現のRed/Blue→3/5 |

C関数名は独立export symbolへ、C型名は独立ABIのtype名へ対応付ける。正常値は1000回反復する。不正tagのhookは試験末尾の意図した呼出し時だけ成功を返すため、正常ループ途中の予期しないNoMatchを成功として隠さない。

検証command:

```sh
cargo test -p emela-emb-compiler --lib backend::lower::tests::p7_data --locked --offline
cargo clippy -p emela-emb-compiler --all-targets --locked --offline -- -D warnings
```

2testが成功し、内部helperはC11 `-Wall -Werror -O0/-O2` の両方でcompile/link/実行する。compiler不在時のskipはない。strict Clippy・実Rust 1.85.0のworkspace全target check・fmt/diff checkも終了0。正本台帳のstatusはこのcommitでは変更しない。
