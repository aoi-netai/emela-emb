mod support;
use emela_emb_compiler::{Options, Source, compile, compile_sources};

#[test]
fn methods_use_the_defining_module_and_visible_traits_and_evaluate_the_receiver_once() {
    let main = r#"
import Model
import Scores.{Score}
@external(c,"mark")
fn mark(n:Int)->Int
fn make()->Model.Point { Model.Point(value:mark(3)) }
fn adjusted()->Int { 99 }
fn generic[A:Score](value:A)->Int { value.next() }
@export(c,"methods")
pub fn run()->Int {
  answer=make().adjusted(b:mark(2),a:mark(1))
  assert generic(Model.Point(value:4))==5
  assert "  x  ".trim()=="x"
  assert True.show()=="True"
  assert None.or(7)==7
  assert Some(7).is_some()
  assert Int.MIN.to_string()=="-2147483648"
  answer
}
"#;
    let model = r#"
pub type Point(value:Int)
pub fn adjusted(value:Point,a:Int,b:Int)->Int { value.value*100+a*10+b }
fn hidden(value:Point)->Int { value.value }
"#;
    let scores = r#"
import Model
pub trait Score {
  fn score(self)->Int
  fn next(self)->Int { self.score()+1 }
}
impl Model.Point:Score { fn score(self) { self.value } }
"#;
    let output = compile_sources(
        &[
            Source::new("main.emel", main),
            Source::new("model.emel", model),
            Source::new("scores.emel", scores),
        ],
        "main.emel",
        &Options::default(),
    )
    .unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
static int trace;
int32_t mark(int32_t n){trace=trace*10+n;return n;}
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(methods()==312);assert(trace==321);return 0;}
"#,
    );
}

#[test]
fn method_lookup_does_not_import_qualified_traits_or_private_functions_and_rejects_ambiguity() {
    let model = "pub type Point(value:Int)\nfn hidden(value:Point)->Int{value.value}";
    let traits = "import Model\npub trait Score{fn score(self)->Int}\nimpl Model.Point:Score{fn score(self){self.value}}";
    for main in [
        "import Model\nimport Scores\nfn wrong(){Model.Point(1).score()}",
        "import Model\nfn wrong(){Model.Point(1).hidden()}",
    ] {
        let errors = compile_sources(
            &[
                Source::new("main.emel", main),
                Source::new("model.emel", model),
                Source::new("scores.emel", traits),
            ],
            "main.emel",
            &Options::default(),
        )
        .unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.source.as_deref() == Some("main.emel"))
        );
    }
    let ambiguous = "type Point(value:Int)\ntrait First{fn score(self)->Int}\ntrait Second{fn score(self)->Int}\nimpl Point:First{fn score(self){1}}\nimpl Point:Second{fn score(self){2}}\nfn wrong(){Point(1).score()}";
    assert!(
        compile(ambiguous)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("ambiguous"))
    );
    assert!(compile("fn wrong[A](value:A){value.show()}").is_err());
    assert!(compile("type Point(value:Int)\nfn value(p:Point)->Int{p.value}").is_err());
}

#[test]
fn field_type_bounds_are_checked_even_in_unused_data_declarations() {
    for source in [
        "type Ordered[A:Ord](value:A)\ntype Invalid(inner:Ordered[Float])",
        "type Ordered[A:Ord](value:A)\ntype Invalid[A](inner:Ordered[A])",
        "type Unused[A:Unknown](value:A)",
    ] {
        let errors = compile(source).unwrap_err();
        assert!(
            errors
                .iter()
                .all(|error| error.source.as_deref() == Some("main.emel"))
        );
    }
    compile("type Ordered[A:Ord](value:A)\ntype Valid[A:Ord](inner:Ordered[A])").unwrap();
}
