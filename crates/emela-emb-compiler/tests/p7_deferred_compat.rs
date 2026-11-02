#[path = "support/legacy_concurrency.rs"]
mod legacy_concurrency;
mod support;
use emela_emb_compiler::Options;

fn options() -> Options {
    legacy_concurrency::options().with_deferred_capacity(2)
}

#[test]
fn await_preserves_string_and_list_results_past_frame_and_scope_reuse() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/09d77fd35b21027246e2fcd97b6a040b1f6f67972dbf29294f0189b4e212408b.emel"
        ),
        include_str!(
            "compat/p7/concurrency/await_preserves_string_and_list_results_past_frame_and_scope_reuse.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn await_resumes_all_waiters_in_fifo_order_after_completer_finishes() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/440f3d1a7a4f4be0f3deead5487b52c757ed2e0637e58281db7595ad1d49c331.emel"
        ),
        include_str!(
            "compat/p7/concurrency/await_resumes_all_waiters_in_fifo_order_after_completer_finishes.c"
        ),
        &options().with_deferred_capacity(3),
    );
}

#[test]
fn complete_copies_borrowed_string_and_poll_retains_it_past_scope_end() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/acd7bf4e88d35dc246b9197dafb293a1321dbca20def9b883831224d407f65d6.emel"
        ),
        include_str!(
            "compat/p7/concurrency/complete_copies_borrowed_string_and_poll_retains_it_past_scope_end.c"
        ),
        &options().with_deferred_capacity(1).with_string_capacity(16),
    );
}

#[test]
fn complete_stores_once_and_scope_end_breaks_cycles_through_captured_deferreds() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/ff14e44cdb1ff58ac3c87d49f7c9d58579ca038d8a6e64b93b1e295f115b3d96.emel"
        ),
        include_str!(
            "compat/p7/concurrency/complete_stores_once_and_scope_end_breaks_cycles_through_captured_deferreds.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn deferred_constructor_reuses_slots_and_scope_only_programs_omit_the_pool() {
    let source = include_str!(
        "compat/legacy-corpus/sources/990d337c23cea5eac8947205398bf55de0d3652b430aa73dbdeaeab6fe702cf6.emel"
    );
    let output = legacy_concurrency::run(
        source,
        include_str!(
            "compat/p7/concurrency/deferred_constructor_reuses_slots_and_scope_only_programs_omit_the_pool.c"
        ),
        &options().with_deferred_capacity(1),
    );
    assert!(
        output
            .files
            .iter()
            .any(|file| file.path == "emela_embedded.c"
                && file.contents.contains("#define EMELA_DEFERRED_CAPACITY 1"))
    );
    let errors =
        legacy_concurrency::compile(source, &options().with_deferred_capacity(0)).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("deferred_capacity must be positive"))
    );
    let scope = legacy_concurrency::compile(
        include_str!(
            "compat/legacy-corpus/sources/e22d947309209ab57f8767c67e5ebe82e750c20a1eeb131408737c10e07fa2fb.emel"
        ),
        &options().with_deferred_capacity(0),
    ).unwrap();
    assert!(
        !scope
            .files
            .iter()
            .any(|file| file.path == "emela_embedded.c"
                && file.contents.contains("#define EMELA_DEFERRED_CAPACITY"))
    );
}

#[test]
fn deferred_pool_exhaustion_uses_the_defined_defect() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/e262c5165926bb3e8596a9bc7de0dea2686af814e0a0d1cf44b0d961169ade9e.emel"
        ),
        include_str!("compat/p7/concurrency/deferred_pool_exhaustion_uses_the_defined_defect.c"),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn deferred_round_trips_int64_and_float_bits_through_await_and_poll() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/f8d01d1632ef0970c36dcee86f02bcad4da89018bdad911a762324ce7dd2b8a2.emel"
        ),
        include_str!(
            "compat/p7/concurrency/deferred_round_trips_int64_and_float_bits_through_await_and_poll.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn deferred_transfers_tuple_values_and_boolean_flags() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c9ee97f7f942892bb82075c56e6b554a5457cd064de0aa4efad9aa9c56696255.emel"
        ),
        include_str!("compat/p7/concurrency/deferred_transfers_tuple_values_and_boolean_flags.c"),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn deferred_wait_is_a_real_resume_boundary_and_root_end_reclaims_pending_operation() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1ea65aa42e332c1d5dba3070ee1f4d4ce24818a918db8e003819a443a9e9db89.emel"
        ),
        include_str!(
            "compat/p7/concurrency/deferred_wait_is_a_real_resume_boundary_and_root_end_reclaims_pending_operation.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_poll() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c4f1b942172d789f31d4b299a7c8ed81c5990455e90b0a5a9f02cf0c7d211bda.emel"
        ),
        include_str!(
            "compat/p7/concurrency/ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_poll.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_await() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/6454690863d207729427473fed100c1850e4acf4a4007de446e9e8053e10f3ab.emel"
        ),
        include_str!(
            "compat/p7/concurrency/ended_deferred_poll_and_await_reject_leaked_callbacks_after_slot_reuse_await.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn leaked_complete_callback_checks_scope_before_recycled_slot() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/c32ba77cbf93104518ccc366d3388e190b4c912b705fc87202fb1ceb507f7510.emel"
        ),
        include_str!(
            "compat/p7/concurrency/leaked_complete_callback_checks_scope_before_recycled_slot.c"
        ),
        &options().with_deferred_capacity(1),
    );
}

#[test]
fn poll_observes_empty_and_first_value_for_scalar_and_unit_without_yielding() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/a6b444b3e7a1aba4ba52ed9aac3547822f224887e093136ac4b78b07c0dae749.emel"
        ),
        include_str!(
            "compat/p7/concurrency/poll_observes_empty_and_first_value_for_scalar_and_unit_without_yielding.c"
        ),
        &options(),
    );
}

#[test]
fn nested_scope_cancellation_unlinks_middle_tail_and_head_waiters_and_ready_deliveries() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/1eb66f065e0d3a0f1d63c3d4c1cbade1d68435d715e94bd746d06d1efa7187f6.emel"
        ),
        include_str!(
            "compat/p7/concurrency/nested_scope_cancellation_unlinks_middle_tail_and_head_waiters_and_ready_deliveries.c"
        ),
        &options()
            .with_deferred_capacity(3)
            .with_frame_capacity(40)
            .with_task_capacity(1)
            .with_event_capacity(1)
            .with_task_adapter("host_wait", "board_wait_start"),
    );
}
