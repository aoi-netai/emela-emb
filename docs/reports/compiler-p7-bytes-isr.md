# Bytes view の旧 ISR 観測

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の `emit::bytes::bytes_size_and_get_are_heapless_and_keep_lengths_above_u16` は、70000 byte の借用 view、128/255 の unsigned element、負数・末尾外 index の None、NULL の空 view に加え、`size` を ISR entry としてコンパイルする。

既存 `tests/p3_bytes.rs::bytes_views_keep_u32_lengths_and_unsigned_elements_without_a_heap` は同じ source API と C 入力・結果・heap 不要の観測を持つため、重複試験を追加せず `Options::with_isr_entry("size")` を指定した。source 関数 `size` と C alias `byte_size` の区別も維持する。

`CARGO_PROFILE_TEST_OPT_LEVEL=1 cargo test -p emela-emb-compiler --test p3_bytes bytes_views_keep_u32_lengths_and_unsigned_elements_without_a_heap --locked --offline` は 1 passed。生成 C の `-std=c11 -Wall -Werror -O0/-O2` によるコンパイル・実行がともに成功した。fmt と diff check も成功した。
