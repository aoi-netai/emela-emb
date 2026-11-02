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
fn a_root_waits_for_cleanup_of_its_resumed_operation_tail() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/81d18ddb5dea24d4127b4c65595a7d6ca70ff1be98d0c1e1cbe28c91a7149a59.emel"
        ),
        include_str!("compat/p7/cleanup/a_root_waits_for_cleanup_of_its_resumed_operation_tail.c"),
        &options(),
    );
}

#[test]
fn cancellation_preserves_a_suspending_release_and_its_pending_task() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/d35296596f708f8d00f6101a81a571f83908a3364c673fecc17c2def917e64dc.emel"
        ),
        include_str!(
            "compat/p7/cleanup/cancellation_preserves_a_suspending_release_and_its_pending_task.c"
        ),
        &options(),
    );
}

#[test]
fn defects_in_suspend_protect_keep_the_configured_immediate_stop_policy() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c4f75955a8d72702630245b1905b7d5e6968cf7bd9344cb3050493f3ab7b20fd.emel"
        ),
        include_str!(
            "compat/p7/cleanup/defects_in_suspend_protect_keep_the_configured_immediate_stop_policy.c"
        ),
        &options(),
    );
}

#[test]
fn scope_cancellation_runs_nested_releases_before_the_parent_continues() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/e9ebf52f239a15608f71f3d45bf35749986eb71c2581c05cd692160a6d4fbf51.emel"
        ),
        include_str!(
            "compat/p7/cleanup/scope_cancellation_runs_nested_releases_before_the_parent_continues.c"
        ),
        &options(),
    );
}
