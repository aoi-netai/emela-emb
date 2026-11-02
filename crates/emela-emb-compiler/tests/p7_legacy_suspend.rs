#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

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
fn tasks() -> Options {
    Options::default()
        .with_task_adapter("host_wait", "board_wait_start")
        .with_fiber_capacity(2)
        .with_frame_capacity(2)
        .with_frame_bytes(128)
        .with_task_capacity(2)
        .with_event_capacity(2)
}

#[test]
fn completed_roots_release_waiting_operation_tasks_and_ignore_their_queued_events() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/4c10e49f4bef3487611c6adaa88a7ff7014c0e904832b1b008445a8e55b73375.emel"
        ),
        &tasks()
            .with_heap_bytes(2048)
            .with_frame_capacity(4)
            .with_frame_bytes(512)
            .with_task_capacity(1)
            .with_event_capacity(1),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/suspend/completed_roots_release_waiting_operation_tasks_and_ignore_their_queued_events.c"
        ),
    );
}

#[test]
fn immediate_resume_finishes_the_operation_body_before_the_caller_continues() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/e7bc12acf2d0288dd7e6254cbf7315e61d1969cd73d5d6d5ea4a5f74d04f25ef.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/suspend/immediate_resume_finishes_the_operation_body_before_the_caller_continues.c"
        ),
    );
}

#[test]
fn delayed_resume_runs_the_caller_until_its_next_suspension() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/9da09eb6e858d975b6f94803cf383249a3fb5c5ef16704c5b931b84ec5cbff54.emel"
        ),
        &tasks()
            .with_heap_bytes(4096)
            .with_fiber_capacity(6)
            .with_frame_capacity(12)
            .with_frame_bytes(512),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/suspend/delayed_resume_runs_the_caller_until_its_next_suspension.c"
        ),
    );
}

#[test]
fn duplicate_resume_reports_the_operation_and_handler() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/3e6bcaa0e2f1e947a5d3f3ac09aeb1f3f705998c81fe61399a1e78ada76a6677.emel"
        ),
        &Options::default().with_heap_bytes(2048),
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p7/suspend/duplicate_resume_reports_the_operation_and_handler.c"),
    );
}

#[test]
fn suspend_handler_source_waits_and_resumes_into_the_callers_result() {
    let output = fixture(
        include_str!(
            "compat/legacy-corpus/sources/44125cff6f82a291bd652a039b83a0f52c9d49ae541c2f9bb2f6be37abaa52a6.emel"
        ),
        &tasks()
            .with_heap_bytes(2048)
            .with_fiber_capacity(8)
            .with_frame_capacity(12)
            .with_frame_bytes(512),
    );
    support::compile_and_run(
        &output,
        include_str!(
            "compat/p7/suspend/suspend_handler_source_waits_and_resumes_into_the_callers_result.c"
        ),
    );
}
