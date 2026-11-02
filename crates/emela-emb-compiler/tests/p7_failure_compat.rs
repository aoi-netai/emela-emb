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
fn child_failure_closes_scope_and_waits_for_sibling_release_before_escape() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/7991952cce373b9b880f10a18f08e443ea78a7453ff98f65657933352053122d.emel"
        ),
        include_str!(
            "compat/p7/cleanup/child_failure_closes_scope_and_waits_for_sibling_release_before_escape.c"
        ),
        &options().with_task_capacity(3).with_event_capacity(3),
    );
}

#[test]
fn delayed_failure_after_resume_formats_owned_error_after_the_call_frame_returns() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/b56c4906d28eaf1bb75b6dd58eb1639bf39b04cb87c98ebc190767005b6fdf41.emel"
        ),
        include_str!(
            "compat/p7/cleanup/delayed_failure_after_resume_formats_owned_error_after_the_call_frame_returns.c"
        ),
        &options(),
    );
}

#[test]
fn failing_and_infallible_scopes_share_the_fork_operation_abi() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/5e1a226ba42d425614c4d95079a737a34af4ed5d0fe1973b0f4b7d64798e1f00.emel"
        ),
        include_str!(
            "compat/p7/cleanup/failing_and_infallible_scopes_share_the_fork_operation_abi.c"
        ),
        &options().with_heap_bytes(4096).with_frame_capacity(32),
    );
}

#[test]
fn failing_suspend_closure_keeps_error_through_suspending_release() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/40bded01e6920ea481b111aae7ec89f526745e8c7548acd83903805cec5a25a8.emel"
        ),
        include_str!(
            "compat/p7/cleanup/failing_suspend_closure_keeps_error_through_suspending_release.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn operation_failure_after_resume_stops_without_writing_a_stale_error_pointer() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1c516fb6b3201b1c543926c4146c397cf95bcd237fbaa1724f3620aa12006873.emel"
        ),
        include_str!(
            "compat/p7/cleanup/operation_failure_after_resume_stops_without_writing_a_stale_error_pointer.c"
        ),
        &options(),
    );
}

#[test]
fn suspend_failures_cross_calls_and_preserve_cleanup_before_escape() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/dd98c468bc68ec71a9f5ff971073727d099a82322e2dbf0190e20820aa290815.emel"
        ),
        include_str!(
            "compat/p7/cleanup/suspend_failures_cross_calls_and_preserve_cleanup_before_escape.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn suspend_operation_failure_before_resume_reaches_its_call_edge() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/67a72a53d11b6049b80a71b17652ea1d80ac2e6a4fd3b9c75a573e6940f81b95.emel"
        ),
        include_str!(
            "compat/p7/cleanup/suspend_operation_failure_before_resume_reaches_its_call_edge.c"
        ),
        &options().with_heap_bytes(2048),
    );
}

#[test]
fn uncaught_suspend_failure_stops_at_public_boundary_after_release() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c3c4984c83cc9696872d967f7dddf48621e6adb5855f8c492409d2bad9c57046.emel"
        ),
        include_str!(
            "compat/p7/cleanup/uncaught_suspend_failure_stops_at_public_boundary_after_release.c"
        ),
        &options(),
    );
}
