# P7 C binding metadataのIR観測

固定旧 `c34c0aeefdd966d854371b3110db00a47798751d` の `crates/emela-codegen-emb/tests/binding_metadata/mod.rs:4`、`imported_c_bindings_preserve_symbols_for_named_file_and_bundled_modules` を独立IRへ移植した。

旧試験はsource入力を持たず、`HostRef::Global` と `HostRef::Import` のNamed/File/Bundledの4種を構築する。各bindingのC symbolは同じ `read_number` で、4個のexportからそれぞれ42を返す。移植先 `backend/lower/p7_binding_tests.rs::global_named_file_and_bundled_host_metadata_preserve_the_external_c_symbol` は同じmetadataと観測を保持し、C11/O0/O2で1000反復、合計4000回のC関数呼出しも確認する。

新しいsourceのexternal構文・module読取りを導入する試験ではない。IR metadataのNamed/File/BundledでもC symbolを保持し、compilerは `binding.c` や旧bundleの実体を読み込まない。通常/dev依存と通常試験に旧crate/checkout参照を追加していない。正本台帳のstatusはこのcommitでは変更しない。

検証: `cargo test -p emela-emb-compiler --lib backend::lower::tests::p7_bindings --locked --offline` が1pass。C helperは必須の `-std=c11 -Wall -Werror -O0/-O2` でcompile/link/実行し、skipなし。strict Clippy全target、実Rust 1.85.0 workspace全target check、fmt/diff checkも終了0。
