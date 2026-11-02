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
fn list_error_payloads_survive_suspending_cleanup_and_are_dropped_on_cancellation() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/28710f3837c49e15ce2634ca30a18ee389f70232b210eeb4ce827484bf4d882d.emel"
        ),
        include_str!(
            "compat/p7/cleanup/list_error_payloads_survive_suspending_cleanup_and_are_dropped_on_cancellation.c"
        ),
        &options().with_heap_bytes(4096),
    );
}

#[test]
fn native_failure_channels_own_formatted_parse_errors_and_list_payloads() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/144f29d9a0b5e2e919653da31576003d7121621b28ec4473cd9a0fa014d2d37d.emel"
        ),
        include_str!(
            "compat/p7/cleanup/native_failure_channels_own_formatted_parse_errors_and_list_payloads.c"
        ),
        &options().with_heap_bytes(2048),
    );
}

#[test]
fn operation_failure_before_resume_transfers_and_releases_its_string_payload() {
    legacy_concurrency::run(
        include_str!(
            "compat/legacy-corpus/sources/6ee68efefc13d3b7c08e0984eeaa92eba1a893cf38f4a2d43fd1bb577de5a935.emel"
        ),
        include_str!(
            "compat/p7/cleanup/operation_failure_before_resume_transfers_and_releases_its_string_payload.c"
        ),
        &options().with_heap_bytes(4096),
    );
}
