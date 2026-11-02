use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn c_exports_reject_unprovided_ambient_effects() {
    let source = "effect Read {fn get()->Int}\n@export(c,\"unprovided\")\npub fn read()->Int use Read{(use Read).get()}";
    let errors = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Export
                && error.source.as_deref() == Some("main.emel")
                && error.message.contains("required effect")
        ),
        "{errors:?}"
    );
}

#[test]
fn selected_startup_functions_cannot_require_an_ambient_evidence_argument() {
    let source = "effect Read {fn get()->Int}\nfn run()->Int use Read{(use Read).get()}";
    let errors = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096).with_entry("run"),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(
            |error| error.kind == emela_emb_compiler::DiagnosticKind::Backend
                && error.message.contains("parameter")
        ),
        "{errors:?}"
    );
}

#[test]
fn source_effect_declarations_and_capability_operations_keep_requirements_checked() {
    for (source, accepted) in [
        (
            "effect Read {fn get()->Int}\nfn bound[R:Immediate](f:fn()->Int use R)->Int use R{f()}\nfn good()->Int use Read{bound(fn(){(use Read).get()})}",
            true,
        ),
        (
            "effect Wait {suspend fn wait()->Int}\nfn bound[R:Immediate](f:fn()->Int use R)->Int use R{f()}\nfn bad()->Int use Wait{bound(fn(){(use Wait).wait()})}",
            false,
        ),
        (
            "effect Wait {suspend fn wait()->Int}\n@external(c,\"host_callback\")\nfn host(f:fn()->Int use Wait)->Int",
            false,
        ),
        (
            "error Oops\neffect Read {fn get()->Int fails Oops}\nfn read()->Int fails Oops use Read{(use Read).get()}",
            true,
        ),
        (
            "error Oops\neffect Read {fn get()->Int fails Oops}\nfn read()->Int use Read{(use Read).get()}",
            false,
        ),
        (
            "effect State[T] {fn get()->T}\nfn get[T]()->T use State[T]{(use State[T]).get()}\nfn read()->Int use State[Int]{get()}",
            true,
        ),
        (
            "fn apply[A,R:Immediate](f:fn(A)->A use R,value:A)->A use R{f(value)}",
            true,
        ),
        (
            "effect Read {fn get()->Int}\nfn read()->Int use Read{(use Read).get()}",
            true,
        ),
        (
            "effect Read {fn get()->Int}\nfn read()->Int use {}{(use Read).get()}",
            false,
        ),
        (
            "effect Read {fn get()->Int}\nfn read()->Int use Read{(use Read).missing()}",
            false,
        ),
        (
            "effect Read {fn get(value:Int)->Int}\nfn read()->Int use Read{(use Read).get(value:42)}",
            true,
        ),
        (
            "effect Read {fn get(value:Int)->Int}\nfn read()->Int use Read{(use Read).get(wrong:42)}",
            false,
        ),
    ] {
        let result = compile_sources(
            &[Source::new("main.emel", source)],
            "main.emel",
            &Options::default().with_heap_bytes(4096),
        );
        assert_eq!(result.is_ok(), accepted, "{source}\n{result:?}");
    }
}
