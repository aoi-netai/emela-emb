#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};
fn options() -> Options {
    Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(512)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
}
fn fixture(original: &str, options: &Options) -> emela_emb_compiler::Output {
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header.split('(').next().unwrap();
            source.push_str(&format!("@export(c,\"legacy_{name}\")\n"));
        }
        source.push_str(line);
    }
    heap::with_heap_audit(
        compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
            .unwrap_or_else(|errors| panic!("{errors:?}")),
    )
}

#[test]
fn attempt_keeps_the_embedded_policy_of_stopping_immediately_on_defect() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/8ea142e4bf88263cbcba03ddba8878f871ffa8d04f469bb959618188c7ffbd93.emel"
        ),
        &options().with_heap_bytes(8192),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/cause/attempt_keeps_the_embedded_policy_of_stopping_immediately_on_defect.c"
        ),
    );
}

#[test]
fn cause_to_string_uses_error_show_and_caller_output_buffer() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/7387dae863e7f1a97c4f3874ddadc150f412b5e7dde9a2a70c157a1265377003.emel"
        ),
        &options().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p7/cause/cause_to_string_uses_error_show_and_caller_output_buffer.c"),
    );
}

#[test]
fn attempt_captures_sync_failure_and_returns_owned_cause() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/a3c77db970a93187f73e1acda5dac41f5237626da01f82b47e11e396411bcf28.emel"
        ),
        &options().with_heap_bytes(1024),
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p7/cause/attempt_captures_sync_failure_and_returns_owned_cause.c"),
    );
}

#[test]
fn raised_interruption_crosses_sync_calls_and_attempt_without_becoming_a_value() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/159a654c1a344468d5e3a70f248efd5f3b53929258ef0967a3b514643ea985af.emel"
        ),
        &options().with_heap_bytes(8192).with_fiber_capacity(10),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/cause/raised_interruption_crosses_sync_calls_and_attempt_without_becoming_a_value.c"
        ),
    );
}

#[test]
fn cause_kind_reads_failure_without_consuming_the_cause() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/bb9a5869439d8f10967ceb3997ec8ac8a9bcbedcc97962e79d1254ae734d58cf.emel"
        ),
        &options().with_heap_bytes(1024),
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p7/cause/cause_kind_reads_failure_without_consuming_the_cause.c"),
    );
}

#[test]
fn attempt_owns_managed_success_and_releases_discarded_results() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/faada42706e33db2b04fb40b0c4d59c6fa0653ef73fe0c3a1286503445153ca2.emel"
        ),
        &options().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/cause/attempt_owns_managed_success_and_releases_discarded_results.c"
        ),
    );
}

#[test]
fn cause_raise_preserves_the_error_and_runs_native_cleanup_before_escape() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/b29b5560fc869cb93fe7731f35fb1c3eb743761c02d8249ddf9b46a36c2e3ee1.emel"
        ),
        &options().with_heap_bytes(1024),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/cause/cause_raise_preserves_the_error_and_runs_native_cleanup_before_escape.c"
        ),
    );
}

#[test]
fn attempt_waits_for_async_cleanup_before_catching_failure() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/8ccb3f0827c9846e83e7b1833d02e2a2e206626fb074cb4cd05d8ac64b6be0d4.emel"
        ),
        &options().with_heap_bytes(4096),
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p7/cause/attempt_waits_for_async_cleanup_before_catching_failure.c"),
    );
}
