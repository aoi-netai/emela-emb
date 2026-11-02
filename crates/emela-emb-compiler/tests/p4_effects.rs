use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_effect_declarations_and_capability_operations_keep_requirements_checked() {
    for (source, accepted) in [
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
