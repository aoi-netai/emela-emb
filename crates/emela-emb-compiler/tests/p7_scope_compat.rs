//! Original Scope source/C corpus with the fixed resource budgets.
#[path = "support/legacy_concurrency.rs"]
mod legacy;
mod support;
use emela_emb_compiler::Options;
fn options() -> Options {
    legacy::options()
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
}

#[test]
fn scope_starts_children_at_real_suspension_and_reclaims_waiting_tasks() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/7c63a85ad77ff1e3a964d8ee29bf14b848551723954ad9daa240aa13905a3951.emel"
        ),
        include_str!(
            "compat/p7/concurrency/scope_starts_children_at_real_suspension_and_reclaims_waiting_tasks.c"
        ),
        &options(),
    );
}

#[test]
fn scope_cancels_unstarted_children_before_running_their_function() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/aa29524d35f30fa4e0631820dd16a8240b1e6b8e5bc086e47a22dea6debcc093.emel"
        ),
        include_str!(
            "compat/p7/concurrency/scope_cancels_unstarted_children_before_running_their_function.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn completed_children_and_queued_events_do_not_leak_across_scope_reuse() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/7c63a85ad77ff1e3a964d8ee29bf14b848551723954ad9daa240aa13905a3951.emel"
        ),
        include_str!(
            "compat/p7/concurrency/completed_children_and_queued_events_do_not_leak_across_scope_reuse.c"
        ),
        &options(),
    );
}

#[test]
fn closing_outer_scope_reclaims_nested_scopes_and_their_tasks() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/01ee0403a9801092d42c75ea68594adfe7debde90b3c457763dbeb99a4244044.emel"
        ),
        include_str!(
            "compat/p7/concurrency/closing_outer_scope_reclaims_nested_scopes_and_their_tasks.c"
        ),
        &options()
            .with_heap_bytes(16384)
            .with_task_capacity(3)
            .with_event_capacity(3),
    );
}

#[test]
fn children_can_fork_siblings_and_inherit_the_scope_call_evidence() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/7a713f19178fbb71a4706bcc3332d37e5e3ee4181aad5facdd89b9d927df1471.emel"
        ),
        include_str!(
            "compat/p7/concurrency/children_can_fork_siblings_and_inherit_the_scope_call_evidence.c"
        ),
        &options().with_heap_bytes(16384),
    );
}

#[test]
fn fork_exhaustion_is_a_fixed_pool_defect_before_children_start() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/3e1f978d4ef410aedf4aae15d6dc9bb139b5dfc30a8e7b00477bbf3aaf68422e.emel"
        ),
        include_str!(
            "compat/p7/concurrency/fork_exhaustion_is_a_fixed_pool_defect_before_children_start.c"
        ),
        &options().with_fiber_capacity(2),
    );
}

#[test]
fn scope_keeps_managed_captures_and_boxes_reference_results_without_leaks() {
    legacy::run(
        include_str!(
            "compat/legacy-corpus/sources/7d2e60e633ebd2a639057570433bdc75c270ca2fb1f9792330c79c781d7d9baf.emel"
        ),
        include_str!(
            "compat/p7/concurrency/scope_keeps_managed_captures_and_boxes_reference_results_without_leaks.c"
        ),
        &options().with_heap_bytes(8192),
    );
}
