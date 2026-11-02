mod support;
use emela_emb_compiler::compile;

const BOARD: &str = r#"
#include <assert.h>
#include <stdlib.h>
#include <stdint.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind kind, const char *message, uint16_t length) { (void)kind; (void)message; (void)length; abort(); }
int main(void) { assert(inference() == 42); return 0; }
"#;

#[test]
fn inferred_top_level_parameters_are_fresh_at_every_call() {
    let output = compile(
        r#"
enum Maybe[A] { None, Some(A) }
fn empty() { None }
fn forward() { empty() }
fn pair[A](value: A) { (value, empty()) }
fn zero() { 0 }
fn is_empty[A](value: Maybe[A]) -> Bool { value match { None -> True
Some(_) -> False } }
fn int_value(value: Maybe[Int]) -> Int { value match { None -> 42
Some(n) -> n } }
fn string_value(value: Maybe[String]) -> String { value match { None -> "empty"
Some(s) -> s } }
@export(c, "inference")
pub fn inference() -> Int {
    unused = None
    assert is_empty(empty())
    a: Maybe[Int] = empty()
    b: Maybe[String] = empty()
    assert int_value(a) == 42 && string_value(b) == "empty"
    assert int_value(forward()) == 42 && string_value(forward()) == "empty"
    number: (Int, Maybe[String]) = pair(17)
    text: (String, Maybe[Int]) = pair("seventeen")
    (n, s) = number
    (t, i) = text
    assert n == 17 && t == "seventeen"
    assert string_value(s) == "empty" && int_value(i) == 42
    zero() + 42
}
"#,
    )
    .unwrap();
    support::compile_and_run(&output, BOARD);
}

#[test]
fn a_recursive_component_generalizes_after_all_member_bodies() {
    let output = compile(
        r#"
enum Maybe[A] { None, Some(A) }
fn first(n: Int) { if n == 0 { None } else { second(n - 1) } }
fn second(n: Int) { if n == 0 { None } else { third(n - 1) } }
fn third(n: Int) { first(n) }
fn explicit[A](a: A, n: Int) { if n == 0 { (a, None) } else { partner(a, n - 1) } }
fn partner[A](a: A, n: Int) { explicit(a, n) }
fn permuted[A](a: A, n: Int) { different(42, a, n) }
fn different[A, B](unused: A, value: B, n: Int) {
    if n == 0 { value } else { permuted(value, n - 1) }
}
fn consume_int(value: Maybe[Int]) -> Int { value match { None -> 42
Some(n) -> n } }
fn consume_string(value: Maybe[String]) -> String { value match { None -> "done"
Some(s) -> s } }
@export(c, "inference")
pub fn inference() -> Int {
    assert consume_int(first(100)) == 42
    assert consume_string(second(100)) == "done"
    p: (Int, Maybe[String]) = explicit(7, 100)
    q: (String, Maybe[Int]) = partner("seven", 100)
    (n, s) = p
    (t, i) = q
    assert n == 7 && t == "seven" && consume_string(s) == "done"
    assert permuted("reordered", 100) == "reordered"
    assert different("ignored", 42, 100) == 42
    consume_int(i)
}
"#,
    )
    .unwrap();
    support::compile_and_run(&output, BOARD);
}

#[test]
fn generalization_keeps_numeric_defaults_rigid_signatures_and_c_boundaries() {
    for source in [
        "fn zero() { 0 }\nfn wide() -> Int64 { zero() }",
        "enum Maybe[A] { None, Some(A) }\n@export(c, \"empty\")\npub fn empty() { None }",
        "enum Maybe[A] { None, Some(A) }\nfn wrong[A](a: A) -> Maybe[A] { Some(42) }",
        "enum Maybe[A] { None, Some(A) }\nfn wrong() { None + 1 }",
        "enum Maybe[A] { None, Some(A) }\nconst EMPTY = None\nfn integer() -> Maybe[Int] { EMPTY }\nfn text() -> Maybe[String] { EMPTY }",
    ] {
        assert!(compile(source).is_err(), "{source}");
    }
}

#[test]
fn an_unconstrained_recursive_result_can_be_instantiated_by_concrete_wrappers() {
    // The inferred result of a nonterminating function is universally quantified.
    // Compile both instances, without executing either infinite recursion.
    let output = compile(
        r#"
fn forever() { forever() }
@export(c, "integer")
pub fn integer() -> Int { forever() }
@export(c, "text")
pub fn text() -> String { forever() }
@export(c, "inference")
pub fn inference() -> Int { 42 }
"#,
    )
    .unwrap();
    support::compile_and_run(&output, BOARD);
}

#[test]
fn recursive_members_only_quantify_variables_present_in_their_own_signatures() {
    let output = compile(
        r#"
enum Maybe[A] { None, Some(A) }
fn first(n: Int) {
  if n == 0 { 42 } else { unused = second(n - 1)
    42 }
}
fn second(n: Int) { first(n)
  None }
@export(c, "inference")
pub fn inference() -> Int {
  unused_int: Maybe[Int] = second(2)
  unused_text: Maybe[String] = second(2)
  first(2)
}
"#,
    )
    .unwrap();
    support::compile_and_run(&output, BOARD);
}

#[test]
fn constructor_fields_infer_nested_generic_results_from_their_declared_type() {
    let source = r#"
enum Assoc[K,V] { Empty
  Entry(K,V) }
type Bag[A](inner: Assoc[A, ()])
fn empty[K,V]() -> Assoc[K,V] { Empty }
fn bag[A]() -> Bag[A] { Bag(inner: empty()) }
type Envelope[A](value: Option[A], spare: Option[Int])
fn missing() { None }
fn envelope[A]() -> Envelope[A] { Envelope(value: missing(), spare: missing()) }
type Wide(value: Int64)
@export(c,"nested_inference")
pub fn nested_inference() -> Int {
  b: Bag[String] = bag()
  e: Envelope[Bool] = envelope()
  assert e.value == None && e.spare == None
  assert Wide(value: 9223372036854775807).value == Int64.MAX
  b.inner match { Empty -> 42
    Entry(_,_) -> 0 }
}
"#;
    let output = emela_emb_compiler::compile(source).unwrap();
    support::compile_and_run(
        &output,
        r#"
#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include "emela_embedded.h"
#include "emela_board.h"
_Noreturn void emela_board_defect(EmelaDefectKind k,const char*m,uint16_t n){(void)k;(void)m;(void)n;abort();}
int main(void){assert(nested_inference()==42);return 0;}
"#,
    );
    assert!(
        emela_emb_compiler::compile(
            "type Wrapper[A](value:A)\nfn wrong[A]() -> Wrapper[A] { Wrapper(value:1) }"
        )
        .is_err()
    );
}
