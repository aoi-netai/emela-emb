//! Original aggregate source/C observations, retaining the old fixed resource budgets.
#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Output, Source, compile_sources};
fn source(id: &str) -> String {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("compat/legacy-corpus/manifest.json")).unwrap();
    let program = manifest["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == format!("program:{id}"))
        .unwrap();
    assert_eq!(program["files"].as_array().unwrap().len(), 1);
    let original = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/compat/legacy-corpus")
            .join(program["files"][0]["fixture"].as_str().unwrap()),
    )
    .unwrap();
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("pub fn ") {
            let name = rest.split('(').next().unwrap();
            source.push_str(&format!("@export(c,\"legacy_{name}\")\n"));
        }
        source.push_str(line);
    }
    source
}
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
fn fixture(id: &str, options: &Options) -> Output {
    let source = source(id);
    let mut output = compile_sources(&[Source::new("main.emel", &source)], "main.emel", options)
        .unwrap_or_else(|e| panic!("{id}: {e:#?}"));
    if output.c_source.contains("emela_heap_storage") {
        output = heap::with_heap_audit(output);
    }
    if output.c_source.contains("emela_fiber_get(") {
        output
            .files
            .iter_mut()
            .find(|f| f.path == "emela_embedded.c")
            .unwrap()
            .contents
            .push_str("\nvoid test_cancel(EmelaFiber h){emela_fiber_get(h)->cancelled=1;}\n");
    }
    output
}
fn header(output: &Output) -> &str {
    &output
        .files
        .iter()
        .find(|f| f.path == "emela_embedded.h")
        .unwrap()
        .contents
}
fn type_id(output: &Output, name: &str, prefix: &str) -> usize {
    let line = header(output)
        .lines()
        .find(|s| s.contains(&format!(" legacy_{name}(")))
        .unwrap_or_else(|| panic!("missing {name}: {}", header(output)));
    number(line, prefix)
}
fn number(text: &str, prefix: &str) -> usize {
    text.split(prefix)
        .nth(1)
        .unwrap()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap()
}
fn release(output: &Output, ty: usize) -> String {
    header(output)
        .lines()
        .find(|s| {
            s.starts_with("void emela_embedded_release_value_")
                && s.contains(&format!("(emela_type_{ty} *value)"))
        })
        .unwrap()
        .strip_prefix("void ")
        .unwrap()
        .split('(')
        .next()
        .unwrap()
        .to_owned()
}
fn run(output: &Output, harness: &str, substitutions: &[(&str, String)]) {
    let mut harness = harness.to_owned();
    for (name, value) in substitutions {
        harness = harness.replace(&format!("{{{name}}}"), value);
    }
    support::compile_and_run(output, &harness);
}

#[test]
fn deferred_complete_poll_and_await_share_owned_string_aggregates_past_scope_end() {
    let output = fixture(
        "c3c4cdd26e1efbbfdf88",
        &Options::default()
            .with_heap_bytes(4096)
            .with_fiber_capacity(8)
            .with_frame_capacity(24)
            .with_frame_bytes(768)
            .with_string_capacity(64)
            .with_deferred_capacity(1),
    );
    let ty = type_id(&output, "head", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/deferred_complete_poll_and_await_share_owned_string_aggregates_past_scope_end.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn native_closure_captures_string_tuple_enum_and_list_after_factory_stack_returns() {
    let output = fixture(
        "ac90e395370d309144f0",
        &options().with_string_capacity(64).with_frame_bytes(768),
    );
    let callback = type_id(&output, "maker", "emela_function_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/native_closure_captures_string_tuple_enum_and_list_after_factory_stack_returns.c"
        ),
        &[("callback", callback.to_string())],
    );
}

#[test]
fn string_aggregates_own_borrowed_inputs_and_native_results_across_suspend_and_cancellation() {
    let output = fixture(
        "ac90e395370d309144f0",
        &options().with_string_capacity(64).with_frame_bytes(768),
    );
    let ty = type_id(&output, "make", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/string_aggregates_own_borrowed_inputs_and_native_results_across_suspend_and_cancellation.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn suspended_child_captures_and_scope_results_keep_string_aggregates_until_last_release() {
    let output = fixture(
        "ac90e395370d309144f0",
        &options().with_string_capacity(64).with_frame_bytes(768),
    );
    let ty = type_id(&output, "make", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/suspended_child_captures_and_scope_results_keep_string_aggregates_until_last_release.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn both_returns_nested_string_aggregates_and_cancels_pending_children_without_leaks() {
    let output = fixture(
        "01353e00d829543bae73",
        &options()
            .with_heap_bytes(4096)
            .with_string_capacity(64)
            .with_frame_bytes(1024),
    );
    let ty = type_id(&output, "both_start", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/both_returns_nested_string_aggregates_and_cancels_pending_children_without_leaks.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn scope_returns_string_enum_exits_after_success_or_interruption_and_frame_reuse() {
    let output = fixture(
        "01353e00d829543bae73",
        &options()
            .with_heap_bytes(4096)
            .with_string_capacity(64)
            .with_frame_bytes(1024),
    );
    let ty = type_id(&output, "terminal_start", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/scope_returns_string_enum_exits_after_success_or_interruption_and_frame_reuse.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn handler_resume_accepts_a_string_enum_and_reclaims_it_when_cancelled_in_either_wait() {
    let output = fixture(
        "01353e00d829543bae73",
        &options()
            .with_heap_bytes(4096)
            .with_string_capacity(64)
            .with_frame_bytes(1024),
    );
    let ty = type_id(&output, "echoed_start", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/handler_resume_accepts_a_string_enum_and_reclaims_it_when_cancelled_in_either_wait.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}

#[test]
fn both_propagates_managed_failure_from_aggregate_callbacks_and_reclaims_its_sibling() {
    let output = fixture(
        "314587c327ffac8b616b",
        &options()
            .with_heap_bytes(4096)
            .with_string_capacity(64)
            .with_frame_bytes(2048),
    );
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/both_propagates_managed_failure_from_aggregate_callbacks_and_reclaims_its_sibling.c"
        ),
        &[],
    );
}

#[test]
fn native_aggregate_callbacks_own_results_after_captures_and_arguments_are_released() {
    let output = fixture(
        "4be591d3835e38a1021a",
        &Options::default()
            .with_heap_bytes(2048)
            .with_string_capacity(64),
    );
    let packet = type_id(&output, "native", "emela_type_");
    let callback = type_id(&output, "maker", "emela_function_type_");
    let definition = header(&output)
        .split(&format!("struct emela_function_type_{callback} {{"))
        .nth(1)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap();
    let pair = number(definition.split("(*call)").nth(1).unwrap(), "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/native_aggregate_callbacks_own_results_after_captures_and_arguments_are_released.c"
        ),
        &[
            ("packet", packet.to_string()),
            ("callback", callback.to_string()),
            ("pair", pair.to_string()),
            ("release", release(&output, packet)),
        ],
    );
}

#[test]
fn heapless_callbacks_accept_and_return_aggregates_containing_function_values() {
    let output = fixture("927b8a0eea79b7bd9ced", &Options::default());
    assert!(!output.c_source.contains("emela_heap_storage"));
    let ty = type_id(&output, "work", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/heapless_callbacks_accept_and_return_aggregates_containing_function_values.c"
        ),
        &[("ty", ty.to_string())],
    );
}

#[test]
fn failing_indirect_aggregate_calls_release_discarded_successes_and_preserve_string_errors() {
    let output = fixture(
        "fce8a13404a3ab9ab16c",
        &Options::default()
            .with_heap_bytes(2048)
            .with_string_capacity(64),
    );
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/failing_indirect_aggregate_calls_release_discarded_successes_and_preserve_string_errors.c"
        ),
        &[],
    );
}

#[test]
fn native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_borrowed()
 {
    let output = fixture("158234f97251d7d95674", &Options::default());
    assert!(!output.c_source.contains("emela_heap_storage"));
    let ty = type_id(&output, "identity", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_borrowed.c"
        ),
        &[("ty", ty.to_string())],
    );
}

#[test]
fn native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_owned()
{
    let text = source("660953efd0834142cd9a");
    let errors = compile_sources(
        &[Source::new("main.emel", &text)],
        "main.emel",
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("heap_bytes = 0")),
        "{errors:#?}"
    );
    let output = fixture(
        "660953efd0834142cd9a",
        &Options::default().with_heap_bytes(256),
    );
    let ty = type_id(&output, "encoded", "emela_type_");
    run(
        &output,
        include_str!(
            "compat/p7/concurrency/native_string_enum_producers_use_owned_backing_and_heapless_borrowed_tuples_stay_borrowed_owned.c"
        ),
        &[("ty", ty.to_string()), ("release", release(&output, ty))],
    );
}
