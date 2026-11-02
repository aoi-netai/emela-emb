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
fn a_direct_suspend_call_can_return_an_inline_exit_into_its_caller_frame() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/a5840e3470614f66980344f3ac02ffad6e9efcdcb84fbd7444bc56d5f7d8f91b.emel"
        ),
        include_str!(
            "compat/p7/concurrency/a_direct_suspend_call_can_return_an_inline_exit_into_its_caller_frame.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_keeps_owned_list_results_after_the_scope_ends() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/a1e7162e9e23d75580b07da1e4b845ebef25f75a9e147d8a2cfa8dc6b7228f9c.emel"
        ),
        include_str!("compat/p7/concurrency/await_keeps_owned_list_results_after_the_scope_ends.c"),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_preserves_int64_float_bits_and_bool_payloads() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/ef337966432b61a2420fa1a0ae2fbbb27583db77585366a4ecd5809ce69b3342.emel"
        ),
        include_str!("compat/p7/concurrency/await_preserves_int64_float_bits_and_bool_payloads.c"),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_returns_success_and_can_mix_with_repeated_join_without_yielding() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c4618bb638c5ba2327260f2fd48651461c415839afd6ecb66bb686aa63ea585e.emel"
        ),
        include_str!(
            "compat/p7/concurrency/await_returns_success_and_can_mix_with_repeated_join_without_yielding.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_with_a_string_exit_keeps_owned_backing_after_scope_and_frame_reuse() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/d0e3146cfb9996adec8328dce5bde501e8a36f30b778415365cf8ac0d7eb5a80.emel"
        ),
        include_str!(
            "compat/p7/concurrency/await_with_a_string_exit_keeps_owned_backing_after_scope_and_frame_reuse.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_await() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/5a0f57185295f477b99708ffcbc95c026b070ebd428ba3103d80cb036160297a.emel"
        ),
        include_str!(
            "compat/p7/concurrency/child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_await.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_join() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/f86ddeba9b739002623e7ae0ff9d503ed9d716ab0b694175ebd40ff38a1a67c5.emel"
        ),
        include_str!(
            "compat/p7/concurrency/child_wait_is_a_resume_boundary_and_scope_end_reclaims_the_operation_tail_join.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn leaked_await_callback_checks_the_ended_scope_before_using_the_child_result() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/f952bd902b1791b304e8bab738848317de14f6447e8db94bbc1afcb41dd7377a.emel"
        ),
        include_str!(
            "compat/p7/concurrency/leaked_await_callback_checks_the_ended_scope_before_using_the_child_result.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn returning_await_exit_through_scope_owns_the_exit_after_scope_end() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/f9cc6597af34a37de88c8130f1c8d40f13e34f26c85e3c475451fca56e647f17.emel"
        ),
        include_str!(
            "compat/p7/concurrency/returning_await_exit_through_scope_owns_the_exit_after_scope_end.c"
        ),
        &options(),
    );
}

#[test]
fn scope_end_breaks_cycles_when_a_child_result_closure_captures_its_own_handle() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/25dde90a27b25bd4a5889318ee9ec9dd2fa236e641c6aa68d3afe3a212a99e7a.emel"
        ),
        include_str!(
            "compat/p7/concurrency/scope_end_breaks_cycles_when_a_child_result_closure_captures_its_own_handle.c"
        ),
        &options().with_heap_bytes(4096).with_deferred_capacity(1),
    );
}

#[test]
fn closing_nested_scopes_unlinks_join_waiters_before_the_target_completes() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/3fb18ffcc526741428327aa1598b208a04597bf9a1ad491b13c4cda41b6bf684.emel"
        ),
        include_str!(
            "compat/p7/concurrency/closing_nested_scopes_unlinks_join_waiters_before_the_target_completes.c"
        ),
        &options().with_frame_capacity(48).with_deferred_capacity(3),
    );
}

#[test]
fn join_preserves_int64_float_bits_and_bool() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/abc7a58c879a7c589b1c621490a800916c29d3df055cf4cb0814be52d5b594b6.emel"
        ),
        include_str!("compat/p7/concurrency/join_preserves_int64_float_bits_and_bool.c"),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn join_results_own_strings_and_lists_after_scope_and_frame_reuse() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1ad3549206b5ee0c2856da0ff8c4b72c2c1ef668cdc5d1a6af82debbafb5a48b.emel"
        ),
        include_str!(
            "compat/p7/concurrency/join_results_own_strings_and_lists_after_scope_and_frame_reuse.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn join_reuses_completed_results_without_yielding_and_waits_for_discarded_unit() {
    let output = legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/4e16d6fb423d21a9b45128415809cc4ecdc97f12ceb845216bb531dfc12a7619.emel"
        ),
        include_str!(
            "compat/p7/concurrency/join_reuses_completed_results_without_yielding_and_waits_for_discarded_unit.c"
        ),
        &options().with_heap_bytes(4096),
    );
    assert!(
        output
            .files
            .iter()
            .any(|file| file.contents.contains("#define EMELA_CHILD_WAIT_RUNTIME"))
    );
    assert!(
        !output
            .files
            .iter()
            .any(|file| file.contents.contains("#define EMELA_DEFERRED_CAPACITY"))
    );
}

#[test]
fn leaked_join_callback_rejects_an_ended_scope_after_reusing_execution_slots() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/bd8b968875e5d3ab2d5c229624b82ca88014d925ae9c5202611944dd8d1e1226.emel"
        ),
        include_str!(
            "compat/p7/concurrency/leaked_join_callback_rejects_an_ended_scope_after_reusing_execution_slots.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn await_and_join_share_fifo_waiters_and_transfer_each_result() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1cb66d19500df0d547f7480e5d52bee20c1b15d63f1627246aee79ffc591e7a6.emel"
        ),
        include_str!(
            "compat/p7/concurrency/await_and_join_share_fifo_waiters_and_transfer_each_result.c"
        ),
        &options().with_frame_capacity(40),
    );
}

#[test]
fn closing_a_scope_drops_ready_join_and_await_frames_before_result_delivery() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/90178b14fd7587b8ac7e63044a6378abc08f0d58fed107a8569454a34d434fd8.emel"
        ),
        include_str!(
            "compat/p7/concurrency/closing_a_scope_drops_ready_join_and_await_frames_before_result_delivery.c"
        ),
        &options().with_frame_capacity(48).with_deferred_capacity(3),
    );
}

#[test]
fn join_wakes_multiple_waiters_in_fifo_after_the_childs_last_breath() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/ee721482a151acf7e2dc7640812dcc4865b0942afffb9d80a97039673d65c54d.emel"
        ),
        include_str!(
            "compat/p7/concurrency/join_wakes_multiple_waiters_in_fifo_after_the_childs_last_breath.c"
        ),
        &options().with_frame_capacity(40),
    );
}
