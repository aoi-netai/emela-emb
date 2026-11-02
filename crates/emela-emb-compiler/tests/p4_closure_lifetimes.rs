#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_closures_preserve_evaluation_order_and_own_borrowed_and_failed_captures() {
    let output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p4/closures.emel"),
        )],
        "main.emel",
        &Options::default()
            .with_heap_bytes(16384)
            .with_string_capacity(256),
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    support::compile_and_run(
        &heap::with_heap_audit(output),
        include_str!("compat/p4/closures-harness.c"),
    );
}
