use emela_emb_compiler::{Options, Source, compile_sources};
#[test]
fn source_scopes_accept_children_with_different_error_and_effect_subsets() {
    for source in [
        include_str!("compat/p6/closing-ready.emel"),
        include_str!("compat/p6/nested-cancel.emel"),
        include_str!("compat/p6/interrupt-waiters.emel"),
        include_str!("compat/p6/completed-children.emel"),
    ] {
        compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default()
                .with_heap_bytes(131072)
                .with_frame_bytes(1024)
                .with_task_adapter("host_wait", "board_wait_start"),
        )
        .unwrap_or_else(|errors| panic!("{errors:?}\n{source}"));
    }
}
