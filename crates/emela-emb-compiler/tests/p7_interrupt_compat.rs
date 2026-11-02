#[path = "support/legacy_concurrency.rs"]
mod legacy_concurrency;
mod support;
use emela_emb_compiler::Options;
fn options() -> Options {
    legacy_concurrency::options()
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
}

#[test]
fn await_of_interrupted_children_returns_owned_failures_repeatedly_and_for_unit() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1727c81927e037fb48ddc111aa207f3e6018d39e69ae7d67bf9f6bb60b98eb01.emel"
        ),
        include_str!(
            "compat/p7/cleanup/await_of_interrupted_children_returns_owned_failures_repeatedly_and_for_unit.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_waiters_receive_failure_after_interrupt_cleanup_before_interrupt_waiters() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/e12af51e4e6d642d31b69735c4649c9e4074b8b36f3ab195b4dfd2e61033f66b.emel"
        ),
        include_str!(
            "compat/p7/cleanup/await_waiters_receive_failure_after_interrupt_cleanup_before_interrupt_waiters.c"
        ),
        &options().with_frame_capacity(40).with_deferred_capacity(1),
    );
}

#[test]
fn interrupt_cancels_unstarted_children_and_does_not_yield_for_ended_children() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/ce6868d5612c00ccc0c4183dfbb4c01455f36bc3f4516bdeabad475f5e1c60bd.emel"
        ),
        include_str!(
            "compat/p7/cleanup/interrupt_cancels_unstarted_children_and_does_not_yield_for_ended_children.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn interrupt_waits_for_suspending_cleanup_and_ignores_old_task_notifications() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/be2503778cb8c1e1d2e76ec65af0a0a5c79c73b3bd60355eb8c8efed7b5a79d2.emel"
        ),
        include_str!(
            "compat/p7/cleanup/interrupt_waits_for_suspending_cleanup_and_ignores_old_task_notifications.c"
        ),
        &options(),
    );
}

#[test]
fn interruption_causes_survive_scope_and_heap_reuse_and_can_be_released_by_the_caller() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/7191948e46203adae86a8958c6a59a5619e7350cae16dab7e3ca08575eb281a0.emel"
        ),
        include_str!(
            "compat/p7/cleanup/interruption_causes_survive_scope_and_heap_reuse_and_can_be_released_by_the_caller.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn join_of_an_interrupted_child_is_the_defined_panic_defect() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/e81f3c1a5c2526a9c2f382cf1e74f2b31147b0a401bf5a525c5db97e1dca7cf4.emel"
        ),
        include_str!(
            "compat/p7/cleanup/join_of_an_interrupted_child_is_the_defined_panic_defect.c"
        ),
        &options(),
    );
}

#[test]
fn leaked_interrupt_callback_reports_ended_scope_after_slot_reuse() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/431dfd4502720b826ee92efd51cd7216b5c155e97cee4a7cec490d39bdb4ed65.emel"
        ),
        include_str!(
            "compat/p7/cleanup/leaked_interrupt_callback_reports_ended_scope_after_slot_reuse.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn multiple_interrupt_waiters_resume_fifo_after_release_finishes() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/6570121a20b232ff437847c3abe2efc1e8a8c6f7be07327d50c7982842f988de.emel"
        ),
        include_str!(
            "compat/p7/cleanup/multiple_interrupt_waiters_resume_fifo_after_release_finishes.c"
        ),
        &options().with_frame_capacity(48),
    );
}

#[test]
fn operation_tails_do_not_keep_a_copy_of_an_expired_cleanup_mask() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/8e1b8d2033236bbd122661d0b5aae6d8fea88156eb3c0088e6096458a425e299.emel"
        ),
        include_str!(
            "compat/p7/cleanup/operation_tails_do_not_keep_a_copy_of_an_expired_cleanup_mask.c"
        ),
        &options(),
    );
}
