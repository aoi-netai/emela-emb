mod support;
use emela_emb_compiler::{Options, Source, compile_sources};

const LIBRARY: &str = r#"
pub type Value(n: Int)
pub trait Scored { fn score(self) -> Int }
impl Value: Scored { fn score(self) { self.n } }
pub fn value() -> Value { Value(42) }
fn private() -> Int { 0 }
"#;

#[test]
fn selective_imports_keep_the_module_namespace_and_allow_repeated_definitions() {
    let source = r#"
import Data.Alpha.{Value, value}
import Data.Alpha.{Value, value}
import Data.Alpha
fn read(v: Alpha.Value) -> Int { Alpha.Scored.score(v) }
@export(c,"qualified")
pub fn qualified() -> Int {
    assert read(Alpha.Value(42)) == read(value())
    read(Alpha.value())
}
"#;
    let output = compile_sources(
        &[
            Source::new("main.emel", source),
            Source::new("data/alpha.emel", LIBRARY),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(qualified()==42);return 0;}
"#,
    );
}

#[test]
fn selective_imports_keep_private_and_unlisted_names_out_of_scope() {
    for body in [
        "fn main(){value()}",
        "fn main(){Alpha.private()}",
        "fn main(v:Value){Scored.score(v)}",
    ] {
        let source = format!("import Data.Alpha.{{Value}}\n{body}");
        assert!(
            compile_sources(
                &[
                    Source::new("main.emel", &source),
                    Source::new("data/alpha.emel", LIBRARY)
                ],
                "main.emel",
                &Options::default(),
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn distinct_modules_and_values_cannot_share_an_imported_name() {
    for (source, second_name) in [
        (
            "import Data.Alpha.{Value}\nimport Other.Alpha.{Value}\nfn main(){0}",
            "other/alpha.emel",
        ),
        (
            "import Data.Alpha.{value}\nimport Data.Beta.{value}\nfn main(){0}",
            "data/beta.emel",
        ),
    ] {
        assert!(
            compile_sources(
                &[
                    Source::new("main.emel", source),
                    Source::new("data/alpha.emel", LIBRARY),
                    Source::new(second_name, LIBRARY)
                ],
                "main.emel",
                &Options::default(),
            )
            .is_err(),
            "{source}"
        );
    }
}
