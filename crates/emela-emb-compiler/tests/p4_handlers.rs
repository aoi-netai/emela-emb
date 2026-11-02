mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

#[test]
fn source_handlers_layers_labels_and_failed_initializers_execute_with_cleanup() {
    let mut output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p4/handlers.emel"),
        )],
        "main.emel",
        &Options::default().with_heap_bytes(16384),
    )
    .unwrap();
    output.files.iter_mut().find(|file| file.path == "emela_embedded.c").unwrap().contents.push_str(
        "\nint test_heap_empty(void){for(unsigned char*p=emela_heap_storage;emela_heap_initialized&&p<emela_heap_end();){emela_heap_block*b=(emela_heap_block*)(void*)p;if(!b->is_free)return 0;p+=b->size;}return 1;}\n");
    support::compile_and_run(&output, include_str!("compat/p4/handlers-harness.c"));
}

#[test]
fn lexical_providers_override_invocation_evidence_and_pure_handlers_need_no_heap() {
    let source = r#"
effect Read { fn read()->Int }
handler One: Read { fn read(){1} }
handler Two: Read { fn read(){2} }
fn read()->Int use Read {(use Read).read()}
@export(c,"lexical_provider")
pub fn value()->Int {with One {callback=fn(){read()}
with Two {callback()}}}
@export(c,"pure_provider")
pub fn pure()->Int {with One {(use Read).read()+41}}
"#;
    let output = compile_sources(
        &[Source::new("main.emel", source)],
        "main.emel",
        &Options::default().with_heap_bytes(4096),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){for(int i=0;i<1000;i++)if(lexical_provider()!=1||pure_provider()!=42)return 1;return 0;}\n",
    );
    let pure = "effect Read {fn read()->Int}\nhandler One:Read{fn read(){42}}\n@export(c,\"pure_provider\")\npub fn pure()->Int{with One{(use Read).read()}}";
    let output = compile_sources(
        &[Source::new("main.emel", pure)],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    assert!(!output.c_source.contains("emela_heap_allocate("));
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){return pure_provider()==42?0:1;}\n",
    );
}

#[test]
fn provider_cycles_label_mismatches_and_layer_impurity_are_rejected() {
    for source in [
        "effect Read{fn read()->Int}\nhandler H:Read{init(n:Int){H}\nfn read(){0}}\nfn f(){with H(n:true),H(n:1){()}}",
        "effect Read{fn read()->Int}\nhandler H:Read{fn read(){0}}\nfn f(){with H(){()}}",
        "effect Read{fn read()->Int}\nhandler H:Read{fn read(){0}}\neffect Label of Read\nfn f(){with Label by H,Label by H{()}}",
        "effect A{fn a()->Int}\neffect B{fn b()->Int}\nhandler H:A{fn a(){0}}\neffect Label of B\nfn f(){with Label by H{()}}",
        "effect A{fn a()->Int}\neffect B{fn b()->Int}\nhandler H:A{fn a(){(use B).b()}}\nhandler I:B{fn b(){(use A).a()}}\nfn f(){with H,I{()}}",
        "effect Read{fn read()->Int}\nhandler H:Read{init(n:Int){H}\nfn read(){0}}\nlayer L{H(n:(use Read).read())}\nfn f(){with L{()}}",
        "effect Read{fn read()->Int}\nfn f(){with Read{()}}",
    ] {
        assert!(
            compile_sources(
                &[Source::new("main.emel", source)],
                "main.emel",
                &Options::default().with_heap_bytes(4096)
            )
            .is_err(),
            "{source}"
        );
    }
}
