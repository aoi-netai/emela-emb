//! Fixed original source followed by its explicit old Protect cleanup test seam.
use super::*;
fn fixture(original: &str) -> Output {
    let mut source = String::new();
    for line in original.split_inclusive('\n') {
        if let Some(header) = line.strip_prefix("pub fn ") {
            let name = header.split('(').next().unwrap();
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
    let mut program = crate::frontend::lower::lower(&checked);
    super::p7_source_seams::protect_cleanup_call(&mut program);
    let mut output = crate::backend::compile_program(
        program,
        &Options::default()
            .with_heap_bytes(4096)
            .with_fiber_capacity(8)
            .with_frame_capacity(24)
            .with_frame_bytes(512)
            .with_task_capacity(3)
            .with_event_capacity(3)
            .with_task_adapter("host_wait", "board_wait_start"),
    )
    .unwrap();
    let c = &mut output
        .files
        .iter_mut()
        .find(|f| f.path == "emela_embedded.c")
        .unwrap()
        .contents;
    c.push_str("\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    output.c_source = c.clone();
    output
}

#[test]
fn failed_empty_cause_raised_in_cleanup_returns_to_its_failing_scope() {
    let output = fixture(include_str!(
        "../../../tests/compat/legacy-corpus/sources/8b94498c13cd48b86155cc4004a49fe423a003224fc85cd635df7126c4e45e58.emel"
    ));
    execute(
        &output,
        include_str!(
            "../../../tests/compat/p7/cause/failed_empty_cause_raised_in_cleanup_returns_to_its_failing_scope.c"
        ),
        0,
    );
}

#[test]
fn interruption_raised_by_cleanup_replaces_a_body_failure() {
    let output = fixture(include_str!(
        "../../../tests/compat/legacy-corpus/sources/f3550c720a53dd18af434b62d672adaf680dae153873e3c1e827eb53af3c42da.emel"
    ));
    let harness = include_str!(
        "../../../tests/compat/p7/cause/interruption_raised_by_cleanup_replaces_a_body_failure.c"
    )
    .replace("{mode}", "2");
    execute(&output, &harness, 0);
    let output = fixture(include_str!(
        "../../../tests/compat/legacy-corpus/sources/a90022a09a7452ef951b489fd35bbd594629625d61791996f617313880e46e9d.emel"
    ));
    let harness = include_str!(
        "../../../tests/compat/p7/cause/interruption_raised_by_cleanup_replaces_a_body_failure.c"
    )
    .replace("{mode}", "0");
    execute(&output, &harness, 0);
    let output = fixture(include_str!(
        "../../../tests/compat/legacy-corpus/sources/be4f39329c71752d895a792108d3b88864beed64d9f192aaf0b7e8cd843f05ba.emel"
    ));
    let harness = include_str!(
        "../../../tests/compat/p7/cause/interruption_raised_by_cleanup_replaces_a_body_failure.c"
    )
    .replace("{mode}", "1");
    execute(&output, &harness, 0);
}
