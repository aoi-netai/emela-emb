#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};

const CHILDREN: &str = include_str!("compat/p6/completed-children.emel");

fn compile(source: &str, heap_bytes: u32, fibers: u16, deferred: u16) -> Output {
    // Keep legacy bodies byte-exact; only make their public functions explicit C roots.
    let source = source
        .lines()
        .map(|line| {
            if let Some(rest) = line.strip_prefix("pub fn ") {
                let name = rest.split('(').next().unwrap();
                format!("@export(c,\"legacy_{name}\")\n{line}")
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let output = compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default()
            .with_heap_bytes(heap_bytes)
            .with_string_capacity(32)
            .with_frame_bytes(1024)
            .with_frame_capacity(128)
            .with_fiber_capacity(fibers)
            .with_deferred_capacity(deferred)
            .with_task_capacity(2)
            .with_event_capacity(2)
            .with_task_adapter("host_wait", "board_wait_start"),
    )
    .unwrap();
    heap::with_heap_audit(output)
}
#[test]
fn closing_a_source_scope_drops_ready_join_and_await_deliveries() {
    let output = compile(include_str!("compat/p6/closing-ready.emel"), 16384, 16, 3);
    support::compile_and_run(&output, include_str!("compat/p6/closing-ready-harness.c"));
}
#[test]
fn cancelling_nested_source_scopes_unlinks_all_deferred_waiters() {
    let output = compile(include_str!("compat/p6/nested-cancel.emel"), 16384, 16, 3);
    support::compile_and_run(&output, include_str!("compat/p6/nested-cancel-harness.c"));
}
#[test]
fn source_interrupt_waiters_resume_fifo_after_suspending_cleanup() {
    let output = compile(
        include_str!("compat/p6/interrupt-waiters.emel"),
        16384,
        16,
        0,
    );
    support::compile_and_run(
        &output,
        include_str!("compat/p6/interrupt-waiters-harness.c"),
    );
}
fn children(source: &str, heap: u32) -> Output {
    let mut output = compile(source, heap, 4, 0);
    output
        .files
        .iter_mut()
        .find(|file| file.path == "emela_embedded.c")
        .unwrap()
        .contents
        .push_str(include_str!("compat/p6/completed-instrument.c"));
    output
}
#[test]
fn source_scopes_reclaim_completed_children_and_preserve_live_copied_handles() {
    let small = children(CHILDREN, 2048);
    support::compile_and_run(&small, include_str!("compat/p6/completed-rounds-harness.c"));
    support::compile_and_run(
        &small,
        include_str!("compat/p6/completed-discard-harness.c"),
    );
    let kept = children(CHILDREN, 4096);
    support::compile_and_run(&kept, include_str!("compat/p6/completed-kept-harness.c"));

    // Reading an existing local as a statement must preserve its later uses.
    let read = CHILDREN.replace(
        "a = Fiber.join(child)",
        "child\n      a = Fiber.join(child)",
    );
    let kept = children(&read, 4096);
    support::compile_and_run(&kept, include_str!("compat/p6/completed-kept-harness.c"));
}
