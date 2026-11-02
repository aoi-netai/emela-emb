#[path = "support/heap.rs"]
mod heap;
mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn source_record_updates_preserve_type_changes_order_and_ownership() {
    let output = compile_sources(
        &[Source::new(
            "main.emel",
            include_str!("compat/p7/record-updates.emel"),
        )],
        "main.emel",
        &Options::default()
            .with_heap_bytes(16384)
            .with_string_capacity(256),
    )
    .unwrap_or_else(|errors| panic!("{errors:?}"));
    support::compile_and_run(
        &heap::with_heap_audit(output),
        include_str!("compat/p7/record-updates-harness.c"),
    );
}

#[test]
fn invalid_update_targets_fields_and_inherited_types_are_rejected() {
    let cases = [
        (
            "type Record(n:Int)\nfn f(r:Record){Record(1,..r)}",
            "fields must be named",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){Record(other:1,..r)}",
            "unknown record update field",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){Record(n:1,n:2,..r)}",
            "supplied twice",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){Record(..r,n:1)}",
            "base must be the last",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){Record(..r,..r)}",
            "base must be the last",
        ),
        (
            "enum Single{Only(n:Int)}\nfn f(e:Single){Only(n:1,..e)}",
            "requires a type, error, or handler",
        ),
        (
            "type Record(n:Int)\nfn make(n:Int)->Record{Record(n:)}\nfn f(r:Record){make(n:1,..r)}",
            "requires a type, error, or handler",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){r(n:1,..r)}",
            "requires a type, error, or handler",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){(fn(x){x})(n:1,..r)}",
            "requires a type, error, or handler",
        ),
        (
            "type Record(n:Int)\nfn f(r:Record){unknown(n:1,..r)}",
            "undefined name 'unknown'",
        ),
        (
            "type Record(n:Int)\ntype Other(n:Int)\nfn f(s:Other){Record(n:1,..s)}",
            "base must have type 'Record'",
        ),
        (
            "type Duo[A](a:A,b:A)\nfn f(d:Duo[Int]){Duo(a:\"s\",..d)}",
            "cannot unify",
        ),
        (
            "type Pair[A,B](a:A,b:B)\nfn f(d:Pair[Int,Bool])->Pair[String,Int]{Pair(a:\"s\",..d)}",
            "cannot unify",
        ),
        (
            "enum Single{Only(n:Int)}\nfn f(e:Single){e.n}",
            "enum fields must be read through a matching pattern",
        ),
        (
            "enum Single{Only(action:fn()->Int)}\nfn f(e:Single){e.action()}",
            "no accessible method",
        ),
        (
            "effect Read{fn read()->Int}\nhandler Reader(n:Int):Read{init{Reader(n:1)}\nfn read(){x=Reader(n:1,..self)\nx.n}}",
            "only inside its initializer",
        ),
        (
            "effect Read{fn read()->Int}\nhandler Reader:Read{init{Reader(..Reader)}\nfn read(){1}}",
            "requires a type, error, or handler",
        ),
        (
            "effect Read{fn read()->Int}\ntype Box[A](value:A)\nfn f(){Box(value:use Read,..Box(value:1))}",
            "a capability value cannot be stored in data",
        ),
    ];
    for (source, expected) in cases {
        let errors = compile(source).expect_err(source);
        assert!(
            errors.iter().any(|e| e.message.contains(expected)),
            "{source}\n{errors:?}"
        );
        if expected.starts_with("undefined name") {
            assert!(
                !errors
                    .iter()
                    .any(|e| e.message.contains("record update requires"))
            );
        }
    }
}

#[test]
fn opaque_records_can_be_updated_only_in_their_defining_module() {
    let library = "pub opaque type Box(value:Int)\npub fn make()->Box{b=Box(value:1)\nBox(value:42,..b)}\npub fn read(b:Box)->Int{b.value}";
    let output = compile_sources(
        &[
            Source::new("box.emel", library),
            Source::new(
                "main.emel",
                "import Box\n@export(c,\"opaque_update\")\npub fn f()->Int{Box.read(Box.make())}",
            ),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        "#include <stdlib.h>\n#include \"emela_embedded.h\"\n#include \"emela_board.h\"\n_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}\nint main(void){return opaque_update()==42?0:1;}\n",
    );
    let errors = compile_sources(
        &[
            Source::new("box.emel", library),
            Source::new(
                "main.emel",
                "import Box\nfn f(){Box.Box(value:1,..Box.make())}",
            ),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap_err();
    assert!(
        errors.iter().any(|e| e.message.contains("opaque")),
        "{errors:?}"
    );
}
