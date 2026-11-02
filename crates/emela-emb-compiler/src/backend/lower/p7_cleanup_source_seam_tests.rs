//! Original sources with precisely the fixed-old post-source IR testing seams.
use super::*;

fn source_program(original: &str) -> Program {
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header
                .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .next()
                .unwrap();
            source.push_str(&format!("@export(c,\"legacy_{name}\")\n"));
        }
        source.push_str(line);
    }
    let mut modules =
        crate::frontend::modules::collect(&[crate::Source::new("main.emel", &source)], "main.emel")
            .unwrap();
    crate::frontend::consts::expand(&mut modules).unwrap();
    let checked = crate::frontend::check::check(modules).unwrap();
    let mut checked = crate::frontend::specialize::specialize(checked).unwrap();
    crate::frontend::function_abi::project(&mut checked).unwrap();
    crate::frontend::lower::lower(&checked)
}

fn prepend_hold(program: &mut Program) {
    let mut changed = 0;
    for (_, function) in program
        .functions
        .iter_mut()
        .filter(|(_, f)| f.name == "late")
    {
        let body = function.body.unwrap();
        let hold = binding(
            function,
            None,
            Value::Runtime {
                op: RuntimeOp::Hold,
                args: vec![],
            },
        );
        function.seqs[body].stmts.insert(0, hold);
        changed += 1;
    }
    assert_eq!(
        changed, 1,
        "the fixed old late-function seam must match exactly once"
    );
}

fn external_request(program: &mut Program) {
    let mut changed = 0;
    for (_, function) in program
        .functions
        .iter_mut()
        .filter(|(_, f)| f.name == "request")
    {
        assert!(function.suspends && function.fails);
        assert!(function.external.is_none() && function.body.is_some());
        function.body = None;
        function.external = Some(External {
            bindings: vec![ExternalBinding {
                target: "c".into(),
                host: HostRef::Global("host_request".into()),
            }],
        });
        changed += 1;
    }
    assert_eq!(
        changed, 1,
        "the fixed old external request seam must match exactly once"
    );
}

fn run(
    original: &str,
    mut harness: String,
    options: Options,
    protect: bool,
    hold: bool,
    external: bool,
) {
    let mut program = source_program(original);
    if protect {
        super::p7_source_seams::protect_cleanup_call(&mut program);
    }
    if hold {
        prepend_hold(&mut program);
    }
    if external {
        external_request(&mut program);
    }
    let mut output = crate::backend::compile_program(program, &options).unwrap();
    if external {
        let error = output
            .header
            .lines()
            .find(|line| {
                line.trim_start().starts_with("emela_type_")
                    && line.trim_end().ends_with(';')
                    && line.contains(" e")
            })
            .unwrap();
        let mut fields = error.split_whitespace();
        let ty = fields.next().unwrap().trim_start_matches("emela_type_");
        let tag = fields
            .next()
            .unwrap()
            .trim_start_matches('e')
            .trim_end_matches(';');
        harness = harness.replace("{ty}", ty).replace("{tag}", tag);
        output
            .files
            .iter_mut()
            .find(|f| f.path == "emela_embedded.c")
            .unwrap()
            .contents
            .push_str("\nvoid test_cancel(EmelaFiber h){emela_fiber_get(h)->cancelled=1;}\n");
    }
    audit_owned_heap(&mut output);
    execute(&output, &harness, 0);
}

#[test]
fn masked_cleanup_can_await_the_failed_child_before_scope_error_propagates() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(512)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/2c24795089adb02c47248169a3fdbfe3c644f3a7243f1f5525cb0da9ef6a9a06.emel"),
        include_str!("../../../tests/compat/p7/cleanup/masked_cleanup_can_await_the_failed_child_before_scope_error_propagates.c").to_owned(),
        options, true, false, false,
    );
}

#[test]
fn interrupted_cleanup_can_close_a_nested_scope_and_wait_for_its_child_release() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
        .with_deferred_capacity(1);
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/2befbd0806bf5306b0e09ebeeda14219f57d886a5fe31c051a65be7d4443aa13.emel"),
        include_str!("../../../tests/compat/p7/cleanup/interrupted_cleanup_can_close_a_nested_scope_and_wait_for_its_child_release.c").to_owned(),
        options, true, false, false,
    );
}

#[test]
fn interrupted_cleanup_can_wait_on_a_deferred_completed_by_a_sibling() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
        .with_deferred_capacity(3);
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/c9588516a250ea0ab6c1082b0d2525bb227ccf79c4a3b5b93cb448972eaa510a.emel"),
        include_str!("../../../tests/compat/p7/cleanup/interrupted_cleanup_can_wait_on_a_deferred_completed_by_a_sibling.c").to_owned(),
        options, true, false, false,
    );
}

#[test]
fn self_interrupt_unwinds_the_body_and_is_deferred_inside_cleanup() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(512)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
        .with_deferred_capacity(2);
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/454bfd87a42e740e6b26864f747ad6e9611467f5cd39a08a02645bde395fdaf6.emel"),
        include_str!("../../../tests/compat/p7/cleanup/self_interrupt_unwinds_the_body_and_is_deferred_inside_cleanup.c").to_owned(),
        options, true, false, false,
    );
}

#[test]
fn later_failures_are_ordered_owned_notes_shared_by_rethrown_causes() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_fiber_capacity(8)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(4)
        .with_event_capacity(4)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/725fa13e508a2eaa0936c1215dab14af612d1010d9eadd6bb4654e3de81b9ab1.emel"),
        include_str!("../../../tests/compat/p7/cleanup/later_failures_are_ordered_owned_notes_shared_by_rethrown_causes.c").to_owned(),
        options, false, true, false,
    );
}

#[test]
fn note_display_checks_the_callers_remaining_capacity() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(8)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(4)
        .with_event_capacity(4)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!(
            "../../../tests/compat/legacy-corpus/sources/725fa13e508a2eaa0936c1215dab14af612d1010d9eadd6bb4654e3de81b9ab1.emel"
        ),
        include_str!(
            "../../../tests/compat/p7/cleanup/note_display_checks_the_callers_remaining_capacity.c"
        )
        .to_owned(),
        options,
        false,
        true,
        false,
    );
}

#[test]
fn failures_from_cancelled_nested_scopes_become_notes_of_the_outer_first_failure() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(10)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(4)
        .with_event_capacity(4)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/6e6c14d15b01141c28b3b111af84d602b3fe7ca1aa1abcdb32a4254f249e2574.emel"),
        include_str!("../../../tests/compat/p7/cleanup/failures_from_cancelled_nested_scopes_become_notes_of_the_outer_first_failure.c").to_owned(),
        options, false, true, false,
    );
}

#[test]
fn nested_scope_failure_survives_external_interruption_before_and_after_failure() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_fiber_capacity(10)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(4)
        .with_event_capacity(4)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/7fc3ef6ca513d4f89d01795c0c3d5012f2789f16569251147cad2fe7721df2a9.emel"),
        include_str!("../../../tests/compat/p7/cleanup/nested_scope_failure_survives_external_interruption_before_and_after_failure.c").to_owned(),
        options, false, true, false,
    );
}

#[test]
fn settled_nested_scope_failures_keep_their_notes_when_cleanup_raises_interrupt() {
    let options = Options::default()
        .with_heap_bytes(8192)
        .with_fiber_capacity(10)
        .with_frame_capacity(48)
        .with_frame_bytes(512)
        .with_task_capacity(3)
        .with_event_capacity(3)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/aea3643aa96302f947090b9591b3266196a7d14191556bc32e922bdfa2ce2a76.emel"),
        include_str!("../../../tests/compat/p7/cleanup/settled_nested_scope_failures_keep_their_notes_when_cleanup_raises_interrupt.c").to_owned(),
        options, true, true, false,
    );
}

#[test]
fn scope_causes_and_notes_keep_string_payloads_after_frames_and_scopes_end() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_fiber_capacity(8)
        .with_frame_capacity(48)
        .with_frame_bytes(768)
        .with_task_capacity(4)
        .with_event_capacity(4)
        .with_task_adapter("host_wait", "board_wait_start");
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/fe93293c01aee5bed6411ac6b8c42ab08ef816aee792fe3d249cb02782ed303f.emel"),
        include_str!("../../../tests/compat/p7/cleanup/scope_causes_and_notes_keep_string_payloads_after_frames_and_scopes_end.c").to_owned(),
        options, false, true, false,
    );
}

#[test]
fn external_failure_payload_survives_suspending_cleanup_and_later_interruption() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(1536)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
        .with_task_adapter("host_request", "board_request_start")
        .with_string_capacity(256);
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/eb0c2cac3a117558bf3564475ed61f667cbe44dcd7ef3821895e3edb69422f64.emel"),
        include_str!("../../../tests/compat/p7/cleanup/external_failure_payload_survives_suspending_cleanup_and_later_interruption.c").to_owned(),
        options, false, false, true,
    );
}

#[test]
fn external_failures_own_borrowed_strings_and_lists_across_escape_attempt_scope_and_cancel() {
    let options = Options::default()
        .with_heap_bytes(4096)
        .with_fiber_capacity(8)
        .with_frame_capacity(24)
        .with_frame_bytes(1536)
        .with_task_capacity(2)
        .with_event_capacity(2)
        .with_task_adapter("host_wait", "board_wait_start")
        .with_task_adapter("host_request", "board_request_start")
        .with_string_capacity(256);
    run(
        include_str!("../../../tests/compat/legacy-corpus/sources/eb0c2cac3a117558bf3564475ed61f667cbe44dcd7ef3821895e3edb69422f64.emel"),
        include_str!("../../../tests/compat/p7/cleanup/external_failures_own_borrowed_strings_and_lists_across_escape_attempt_scope_and_cancel.c").to_owned(),
        options, false, false, true,
    );
}
