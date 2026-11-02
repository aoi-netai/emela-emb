use emela_emb_compiler::{DiagnosticKind, Options, Source, compile_sources};

#[test]
fn capability_values_and_captured_providers_cannot_escape_their_scope() {
    let declarations =
        "effect Read {fn get()->Int}\nhandler Reader:Read{release{()}\nfn get(){42}}\n";
    for body in [
        "fn bad(){use Read}",
        "fn bad(){(use Read,0)}",
        "fn identity[A](value:A)->A{value}\nfn bad(){identity(use Read)}",
        "fn bad(value:Read){()}",
        "fn bad()->fn()->Int{with Reader{cap=use Read\nfn(){cap.get()}}}",
        "fn identity[A](value:A)->A{value}\nfn bad()->fn()->Int{with Reader{identity(fn(){(use Read).get()})}}",
    ] {
        let source = format!("{declarations}{body}");
        let result = compile_sources(
            &[Source::new("main.emel", &source)],
            "main.emel",
            &Options::default().with_heap_bytes(4096),
        );
        let errors = match result {
            Ok(_) => panic!("unexpectedly accepted: {body}"),
            Err(errors) => errors,
        };
        assert!(
            errors.iter().all(|error| error.kind == DiagnosticKind::Type
                && error.source.as_deref() == Some("main.emel")),
            "{body}\n{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("capability")
                    || error.message.contains("effect declaration")),
            "{body}\n{errors:?}"
        );
    }
    let source = format!(
        "{declarations}fn good()->Int{{with Reader{{callback=fn(){{(use Read).get()}}\nwith Reader{{callback()}}}}}}"
    );
    compile_sources(
        &[Source::new("main.emel", &source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
}
